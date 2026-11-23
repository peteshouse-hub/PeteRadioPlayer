#pragma once

#include <Arduino.h>
#include <time.h>

namespace ProgrammeMetadata {
struct Programme {
  String title;
  time_t start = 0;
  time_t end = 0;
  unsigned generation = 0;
};

// Only recognized BBC playlist paths can trigger a metadata lookup.
String serviceId(const String& stream);
bool fetch(const String& service, time_t when, unsigned generation);
bool take(Programme& programme);
}
