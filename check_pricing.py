#!/usr/bin/env python3
"""Report peak/off-peak billing status for the vendors in resources/pricing_rules.json.

This is the CLI counterpart to the panel applet. It reads the same bundled rules
file so the two cannot disagree about windows, weekdays, holidays or promotions.
"""
import argparse
import json
import os
from datetime import date, datetime, timedelta, timezone

RULES_PATH = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "resources", "pricing_rules.json"
)

# Days scanned for an upcoming transition; mirrors TRANSITION_LOOKAHEAD_DAYS.
TRANSITION_LOOKAHEAD_DAYS = 14


def format_duration(td):
    days = td.days
    hours = td.seconds // 3600
    minutes = (td.seconds % 3600) // 60
    parts = []
    if days > 0:
        parts.append(f"{days}D")
    if hours > 0 or days > 0:
        parts.append(f"{hours}H")
    parts.append(f"{minutes}Min")
    return " ".join(parts)


def parse_exceptions(raw):
    """Normalise "YYYY-MM-DD" and ["START", "END"] entries to (start, end) dates.

    `start` is inclusive and `end` is exclusive.
    """
    out = []
    for entry in raw:
        if isinstance(entry, str):
            start = date.fromisoformat(entry)
            end = start + timedelta(days=1)
        else:
            start, end = (date.fromisoformat(x) for x in entry)
        if end <= start:
            raise ValueError(f"exception range {start}..{end} must be increasing")
        out.append((start, end))
    return out


def load_rules(path=RULES_PATH):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def peak_enabled_on(vendor, day):
    days = vendor.get("days_of_week")
    if not days:
        return True  # key absent: historical "every day" behaviour
    return day.isoweekday() in days


def in_exception(vendor, day):
    return any(
        start <= day < end for start, end in parse_exceptions(vendor.get("exceptions", []))
    )


def off_peak_rate(vendor, now):
    promo = vendor.get("promotion")
    if promo:
        end = datetime.fromisoformat(promo["end_date"].replace("Z", "+00:00"))
        if now < end:
            return promo["off_peak_rate"]
    return vendor["default_rate"]


def check_vendor_pricing(vendor, now):
    """Return (is_peak, rate, reason, next_change, countdown)."""
    activation = vendor.get("activation_date")
    if activation:
        act = datetime.fromisoformat(activation.replace("Z", "+00:00"))
        if now < act:
            return False, vendor["default_rate"], "not-yet-active", act, format_duration(act - now)

    offset = timedelta(hours=vendor["timezone_offset_hours"])
    now_local = now + offset
    today = now_local.date()
    hour = now_local.hour

    is_exception = in_exception(vendor, today)
    current_peak = None
    if peak_enabled_on(vendor, today) and not is_exception:
        for peak in vendor["peaks"]:
            if peak["start_hour"] <= hour < peak["end_hour"]:
                current_peak = peak
                break

    is_peak = current_peak is not None
    if is_peak:
        rate, reason = current_peak["rate"], "peak"
    elif is_exception:
        rate, reason = off_peak_rate(vendor, now), "exception"
    else:
        rate, reason = off_peak_rate(vendor, now), "valley"

    transition_hours = sorted(
        {h for peak in vendor["peaks"] for h in (peak["start_hour"], peak["end_hour"])}
    )
    if not transition_hours:
        nxt = now + timedelta(hours=1)
        return is_peak, rate, reason, nxt, format_duration(nxt - now)

    base_today_local = now_local.replace(minute=0, second=0, microsecond=0)
    next_change = None
    for day_offset in range(TRANSITION_LOOKAHEAD_DAYS):
        base_day_local = base_today_local + timedelta(days=day_offset)
        day = base_day_local.date()
        if not peak_enabled_on(vendor, day) or in_exception(vendor, day):
            continue
        for h in transition_hours:
            trans_utc = base_day_local.replace(hour=h) - offset
            if trans_utc > now and (next_change is None or trans_utc < next_change):
                next_change = trans_utc

    if next_change is None:
        next_change = now + timedelta(hours=1)
    return is_peak, rate, reason, next_change, format_duration(next_change - now)


def main():
    parser = argparse.ArgumentParser(description="Check Peak/Valley Pricing Status")
    parser.add_argument("--time", help="Mock time in ISO format (YYYY-MM-DDTHH:MM:SSZ)")
    parser.add_argument("--rules", default=RULES_PATH, help="Path to pricing_rules.json")
    args = parser.parse_args()

    if args.time:
        try:
            now = datetime.fromisoformat(args.time.replace("Z", "+00:00"))
        except ValueError:
            print("Invalid time format. Please use YYYY-MM-DDTHH:MM:SSZ")
            return
    else:
        now = datetime.now(timezone.utc)

    print(f"Evaluation Time (UTC): {now.strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"Rules: {args.rules}")

    for vendor in load_rules(args.rules)["vendors"]:
        is_peak, rate, reason, next_change, countdown = check_vendor_pricing(vendor, now)
        label = {
            "peak": "📈 PEAK",
            "valley": "📉 VALLEY",
            "exception": "📉 OFF-PEAK (holiday/promo)",
            "not-yet-active": "⏳ NOT YET ACTIVE",
        }[reason]
        local_offset = vendor["timezone_offset_hours"]
        sign = "+" if local_offset >= 0 else "-"
        print("-" * 50)
        print(f"{vendor['icon']} {vendor['name']} API:")
        print(f"  Status: {label} ({rate:.1f}x rate)")
        print(f"  Next change in: {countdown} (at {next_change.strftime('%Y-%m-%d %H:%M:%S UTC')})")
        print(
            "  Local switch: "
            f"{(next_change + timedelta(hours=local_offset)).strftime('%Y-%m-%d %H:%M:%S')} "
            f"(UTC{sign}{local_offset})"
        )


if __name__ == "__main__":
    main()
