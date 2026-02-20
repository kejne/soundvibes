Feature: Socket communication
  @at12
  Scenario: Plain toggle uses configured default language
    Given language is set to "sv" in config
    When I send a toggle command
    Then the daemon should receive "toggle lang=sv"

  @at12a
  Scenario: Control socket toggle with language and status response
    Given a running daemon with mocked backend
    When I send toggle command with language "fr"
    Then the response should contain ok = true
    And the response should contain state = "recording"
    And the response should contain language = "fr"
    When I send status command
    Then the response should contain state = "recording"
    And the response should contain language = "fr"

  @at13
  Scenario: Events socket fans out to multiple clients
    Given a running daemon with mocked backend
    And two connected event subscribers
    When I send toggle command to start recording
    And I send toggle command to stop recording
    Then both subscribers should receive identical events
    And the events should include DaemonReady
    And the events should include ModelLoaded
    And the events should include RecordingStarted
    And the events should include TranscriptFinal
    And the events should include RecordingStopped

  @at14
  Scenario: Set language switches active language and transcript language
    Given a running daemon with mocked backend returning "hej"
    And the daemon language is "en"
    When I send set_language command with "sv"
    Then the response should contain language = "sv"
    When I send toggle command to start recording
    And I send toggle command to stop recording
    Then the model should be reloaded for language "sv"
    And the transcript should have language "sv"
