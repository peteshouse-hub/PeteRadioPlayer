#include "Orientation.h"

#include <Wire.h>

bool OrientationSensor::write(uint8_t reg, uint8_t value) {
  Wire.beginTransmission(address);
  Wire.write(reg);
  Wire.write(value);
  return Wire.endTransmission() == 0;
}

bool OrientationSensor::read(uint8_t reg, uint8_t* data, size_t count) {
  Wire.beginTransmission(address);
  Wire.write(reg);
  if (Wire.endTransmission(false) != 0 || Wire.requestFrom(address, count) != count)
    return false;
  for (size_t i = 0; i < count; ++i) data[i] = Wire.read();
  return true;
}

bool OrientationSensor::begin() {
  if (!Wire.begin(11, 10, 400000)) return false;
  Wire.setTimeOut(12);
  uint8_t identity = 0;
  if (!read(0x00, &identity, 1) || identity != 0x05) {
    address = 0x6A;
    if (!read(0x00, &identity, 1) || identity != 0x05) return false;
  }
  // QMI8658 auto-increment, accelerometer at ±2 g / 60 Hz, then enable it.
  ready = write(0x08, 0x00) && write(0x02, 0x40) &&
          write(0x03, 0x07) && write(0x08, 0x41);
  return ready;
}

DisplayRotation OrientationSensor::update(DisplayRotation current, uint32_t now) {
  if (!ready || now - lastSample < 50) return current;
  lastSample = now;
  uint8_t data[6];
  if (!read(0x35, data, sizeof(data))) {
    candidateSince = 0;
    return current;
  }
  // The sensor is mounted a quarter-turn from the panel’s portrait axes.
  const int x = -static_cast<int16_t>(data[2] | (data[3] << 8));
  const int y = static_cast<int16_t>(data[0] | (data[1] << 8));
  const int z = static_cast<int16_t>(data[4] | (data[5] << 8));
  const float magnitude = sqrtf(static_cast<float>(x) * x + static_cast<float>(y) * y +
                                static_cast<float>(z) * z);

  DisplayRotation next = current;
  if (magnitude >= 11500 && magnitude <= 22000) {
    if (abs(x) >= 8900 && abs(x) > abs(y) + 3000 && abs(x) > abs(z)) {
      next = x > 0 ? DisplayRotation::Right : DisplayRotation::Left;
    } else if (abs(y) >= 8900 && abs(y) > abs(x) + 3000 && abs(y) > abs(z)) {
      next = y < 0 ? DisplayRotation::Portrait : DisplayRotation::Inverted;
    }
  }
  if (next == current) {
    candidateSince = 0;
    return current;
  }
  if (!candidateSince || next != candidate) {
    candidate = next;
    candidateSince = now;
    return current;
  }
  return now - candidateSince >= 400 ? next : current;
}
