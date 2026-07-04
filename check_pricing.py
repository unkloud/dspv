#!/usr/bin/env python3
import argparse
from datetime import datetime, timezone, timedelta

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

def get_deepseek_status(now):
    policy_start = datetime(2026, 7, 15, 0, 0, 0, tzinfo=timezone.utc)
    if now < policy_start:
        first_peak = datetime(2026, 7, 15, 1, 0, 0, tzinfo=timezone.utc)
        return False, 1.0, first_peak, format_duration(first_peak - now)
    
    hour = now.hour
    is_peak = (1 <= hour < 4) or (6 <= hour < 10)
    rate = 2.0 if is_peak else 1.0
    
    base_today = now.replace(minute=0, second=0, microsecond=0)
    if 1 <= hour < 4:
        next_change = base_today.replace(hour=4)
    elif 4 <= hour < 6:
        next_change = base_today.replace(hour=6)
    elif 6 <= hour < 10:
        next_change = base_today.replace(hour=10)
    elif hour < 1:
        next_change = base_today.replace(hour=1)
    else:
        next_change = (base_today + timedelta(days=1)).replace(hour=1)
        
    return is_peak, rate, next_change, format_duration(next_change - now)

def get_glm_status(now):
    hour = now.hour
    # Peak hours: 14:00 - 18:00 UTC+8 (which is 06:00 - 10:00 UTC)
    is_peak = 6 <= hour < 10
    
    # Limited-time benefit: 1x off-peak quota through September 2026 (ends Oct 1 2026 UTC+8, i.e. Sept 30 16:00 UTC)
    september_end = datetime(2026, 9, 30, 16, 0, 0, tzinfo=timezone.utc)
    if is_peak:
        rate = 3.0
    elif now < september_end:
        rate = 1.0
    else:
        rate = 2.0
        
    base_today = now.replace(minute=0, second=0, microsecond=0)
    if 6 <= hour < 10:
        next_change = base_today.replace(hour=10)
    elif hour < 6:
        next_change = base_today.replace(hour=6)
    else:
        next_change = (base_today + timedelta(days=1)).replace(hour=6)
        
    return is_peak, rate, next_change, format_duration(next_change - now)

def main():
    parser = argparse.ArgumentParser(description="Check Peak/Valley Pricing Status")
    parser.add_name = parser.add_argument("--time", help="Mock time in ISO format (YYYY-MM-DDTHH:MM:SSZ)")
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
    print("-" * 50)
    
    ds_peak, ds_rate, ds_change, ds_countdown = get_deepseek_status(now)
    print("DeepSeek API:")
    print(f"  Status: {'📈 PEAK' if ds_peak else '📉 VALLEY'} ({ds_rate:.1f}x rate)")
    print(f"  Next change in: {ds_countdown} (at {ds_change.strftime('%Y-%m-%d %H:%M:%S UTC')})")
    
    print("-" * 50)
    
    glm_peak, glm_rate, glm_change, glm_countdown = get_glm_status(now)
    print("z.ai GLM API (GLM-5.2 / GLM-5-Turbo):")
    print(f"  Status: {'📈 PEAK' if glm_peak else '📉 VALLEY'} ({glm_rate:.1f}x rate)")
    print(f"  Next change in: {glm_countdown} (at {glm_change.strftime('%Y-%m-%d %H:%M:%S UTC')})")

if __name__ == "__main__":
    main()
