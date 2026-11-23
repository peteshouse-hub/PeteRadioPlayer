#pragma once

#include <cstdio>
#include <ctime>

inline time_t bbcUtcTime(const char* value) {
  if (!value) return 0;
  int year, month, day, hour, minute, second;
  if (sscanf(value, "%d-%d-%dT%d:%d:%d", &year, &month, &day, &hour, &minute, &second) != 6 ||
      year < 2020 || year > 2100 || month < 1 || month > 12 || day < 1 || day > 31 ||
      hour < 0 || hour > 23 || minute < 0 || minute > 59 || second < 0 || second > 59) return 0;
  const auto leap = [](int y) { return y % 4 == 0 && (y % 100 != 0 || y % 400 == 0); };
  constexpr int MonthDays[] = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
  int days = day - 1;
  for (int y = 1970; y < year; ++y) days += 365 + leap(y);
  for (int m = 1; m < month; ++m) days += MonthDays[m - 1] + (m == 2 && leap(year));
  return static_cast<time_t>(days) * 86400 + hour * 3600 + minute * 60 + second;
}
