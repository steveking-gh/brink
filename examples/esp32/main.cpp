// Simple LED blinking program to test ESP32 firmware image creation.

#include <Arduino.h>

// On the DevKitM-1, the RGB LED is on pin 48
#define RGB_LED_PIN 48

void setup() {
  Serial.begin(115200);
  delay(1000);  // Short delay to ensure serial is ready

  if (psramInit()) {
    Serial.println("PSRAM is enabled!");
    Serial.printf("PSRAM size: %u bytes\n", ESP.getPsramSize());
  } else {
    Serial.println("PSRAM is not enabled or not found.");
  }
}

void loop() {
    // neopixelWrite(PIN, RED, GREEN, BLUE)
    // Values range from 0 (off) to 255 (max brightness)

    // Turn LED Red
    neopixelWrite(RGB_LED_PIN, 20, 0, 0);
    delay(100);

    // Turn LED Green
    neopixelWrite(RGB_LED_PIN, 0, 20, 0);
    delay(100);

    // Turn LED Blue
    neopixelWrite(RGB_LED_PIN, 0, 0, 20);
    delay(100);

    // Turn LED Off
    neopixelWrite(RGB_LED_PIN, 0, 0, 0);
    delay(100);
  }