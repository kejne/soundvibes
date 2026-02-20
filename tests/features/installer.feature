Feature: Installer operations
  @at11
  Scenario: Installer is idempotent and preserves config
    Given an existing config file with format = "jsonl"
    When I run the installer once
    Then the installation should succeed
    When I run the installer again
    Then the installation should succeed
    And the existing config should be preserved
    And the systemd service should be created

  @at11a
  Scenario: Installer handles Wayland display
    Given WAYLAND_DISPLAY is set to "wayland-0"
    And DISPLAY is empty
    When I run the installer
    Then the installer should detect "Wayland display server detected"

  @at11a
  Scenario: Installer handles X11 display
    Given WAYLAND_DISPLAY is empty
    And DISPLAY is set to ":0"
    When I run the installer
    Then the installer should detect "X11 display server detected"

  @at11a
  Scenario: Installer handles headless environment
    Given WAYLAND_DISPLAY is empty
    And DISPLAY is empty
    When I run the installer
    Then the installer should report "Could not detect display server"

  @at11b
  Scenario: Installer rejects unsupported platform
    Given the platform is Darwin
    When I run the installer
    Then the installer should fail
    And the error should contain "SoundVibes only supports Linux"
