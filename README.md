# RadioKing for Waveshare ESP32-S3-Touch-LCD-2.8 V2

RadioKing streams live radio directly over Wi-Fi and plays through the board’s PCM5101 I²S DAC and speaker. The firmware targets the Rev1.1 / V2 board with the CST3530 touch controller.

The three preset pages follow all 12 international BBC live stations and their order from [RadioKing v0.4.0-alpha.13](https://github.com/peteshouse-hub/PeteRadioPlayer/tree/v0.4.0-alpha.13). Station search uses [Radio Browser](https://docs.radio-browser.info/) and ranks up to 40 results by votes.

## Use

1. On first boot, join **RadioKing-Setup** from a phone or computer. Use the temporary password displayed on the board.
2. Open **http://192.168.4.1** while connected to that network. Enter the name and password of your 2.4 GHz Wi-Fi network. The board saves the credentials after a successful connection and closes its setup network.
3. Use **<** and **>** to page through the BBC stations. Select a station and tap **Play**. Selecting another station during playback retunes the radio. Use **Stop** to end playback and the physical volume knob to adjust the speaker.
4. Tap **Search**, type at least two characters of the station name, and tap **Find**. Select a result just like a preset. **Edit** returns to the keyboard and **BBC** returns to the presets.
5. For a supported BBC station, tap **Rewind** and set the time behind live in 5- or 30-minute steps. Tap **Listen** to start there or **Live** to return to the live broadcast. The available history comes from the station’s rolling playlist and can vary. The screen shows the window once the board has fetched it.
6. Tap **Setup** to view the current Wi-Fi connection or choose **Change Wi-Fi**. Open **Orientation** and choose **Auto**, **Portrait up**, **Portrait down**, **Landscape up**, or **Landscape down**. The setting persists across restarts. Auto follows the board after a short pause.
7. In **Setup**, tap **Audio** to switch between stereo and mono. Stereo is the default. Tap **Dim level** to cycle through 10%, 25%, 50%, 75%, and 100%. The screen dims after five minutes without a touch, and the first tap restores full brightness. Select 100% to keep the screen at full brightness. Both settings persist across restarts.

The main screen shows BBC programme and episode information when available, including during rewind. Other stations can supply a track or show title through the stream. Long labels scroll within their own controls.

Station availability depends on the broadcaster and network. Search and rewind use HTTPS and need the board’s clock to synchronize after connecting to Wi-Fi. Rewind supports the 12 BBC presets and recognized BBC media URLs from search, including BBC Radio Tees. The firmware decodes audio to PCM for the speaker.

## Build and upload

Install Arduino CLI and the Espressif Arduino core:

```sh
arduino-cli core install esp32:esp32@3.3.13 --additional-urls https://espressif.github.io/arduino-esp32/package_esp32_index.json
arduino-cli lib install 'Adafruit GFX Library@1.13.0' 'Adafruit BusIO@1.17.4' 'Adafruit ST7735 and ST7789 Library@1.12.0' 'ArduinoJson@7.4.2'
```

Download the [Waveshare V2 examples](https://files.waveshare.com/wiki/ESP32-S3-Touch-LCD-2.8/ESP32-S3-Touch-LCD-2.8-V2-Demo.zip). Copy `Arduino/libraries/ESP32-audioI2S-master` to the `libraries` folder of your Arduino sketchbook. Copy `Touch_CST3530.cpp` and `Touch_CST3530.h` from `Arduino/examples/LVGL_Arduino` into the `RadioKing` sketch folder. The release build used the V2 archive with SHA-256 `b09e8cdfb47212ea94a06ba02a11568dfb70dcc1ea040142067d08e374b29737`.

```sh
board='esp32:esp32:esp32s3:FlashSize=16M,FlashMode=qio,PSRAM=opi,PartitionScheme=huge_app,USBMode=hwcdc,CDCOnBoot=cdc'
arduino-cli compile --fqbn "$board" --output-dir build RadioKing
arduino-cli upload --fqbn "$board" --port /dev/cu.usbmodem1101 --input-dir build RadioKing
```

Use the serial port reported by `arduino-cli board list` if it changes. A serial terminal at 115200 baud shows startup diagnostics and playback state. Waveshare provides the original demo firmware in its V2 examples archive.

## Release image

Flash the release’s merged image at address `0x0` on the Waveshare Rev1.1 / V2. A merged image covers the entire flash and resets saved Wi-Fi and orientation settings. Use the Arduino upload command above to update just the firmware partitions.

This repository’s RadioKing source is MIT-licensed. The combined firmware includes the GPLv3 ESP32-audioI2S library from the Waveshare archive and is distributed under GPLv3. The build instructions identify the corresponding source and build settings. The Arduino ESP32 core is available from [Espressif](https://github.com/espressif/arduino-esp32/tree/3.3.13). The graphics and JSON libraries are available from the Arduino library registry at the versions listed above.

The embedded DejaVu Sans glyphs retain their [font license](RadioKing/fonts/LICENSE-DejaVu.txt).
