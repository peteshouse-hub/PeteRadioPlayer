#pragma once

#include <Arduino.h>

enum class DisplayRotation : uint8_t { Inverted = 0, Left = 1, Portrait = 2, Right = 3 };

constexpr DisplayRotation oppositeRotation(DisplayRotation rotation) {
  return static_cast<DisplayRotation>((static_cast<uint8_t>(rotation) + 2) % 4);
}

class OrientationSensor {
public:
  bool begin();
  DisplayRotation update(DisplayRotation current, uint32_t now);

private:
  bool write(uint8_t reg, uint8_t value);
  bool read(uint8_t reg, uint8_t* data, size_t count);
  uint8_t address = 0x6B;
  bool ready = false;
  DisplayRotation candidate = DisplayRotation::Portrait;
  uint32_t candidateSince = 0;
  uint32_t lastSample = 0;
};

inline bool isLandscape(DisplayRotation rotation) {
  return rotation == DisplayRotation::Left || rotation == DisplayRotation::Right;
}

inline void rotateTouch(DisplayRotation rotation, int& x, int& y) {
  const int portraitX = x;
  const int portraitY = y;
  switch (rotation) {
    case DisplayRotation::Portrait: break;
    case DisplayRotation::Inverted: x = 239 - portraitX; y = 319 - portraitY; break;
    case DisplayRotation::Left: x = 319 - portraitY; y = portraitX; break;
    case DisplayRotation::Right: x = portraitY; y = 239 - portraitX; break;
  }
}
