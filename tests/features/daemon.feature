Feature: Daemon operations
  Background:
    Given a valid model file exists at the default location

  @at01
  Scenario: Daemon starts with valid model
    Given the config directory is empty
    And download_model is disabled
    When I start the daemon
    Then the daemon should start successfully
    And the daemon should be ready to capture audio

  @at02
  Scenario: Missing model returns exit code 2
    Given the config directory is empty
    And download_model is disabled
    And no model file exists
    When I start the daemon
    Then the daemon should exit with code 2
    And the error should contain "model file not found"

  @at03
  Scenario: Invalid input device returns exit code 3
    Given the config directory is empty
    And download_model is disabled
    And the device is set to "nonexistent"
    When I start the daemon
    Then the daemon should exit with code 3
    And the error should contain "input device not found"

  @at04
  Scenario: Daemon toggle captures and transcribes audio
    Given a mocked audio backend with sample audio
    And a mocked transcriber returning "hello"
    When I send toggle command to start recording
    And I wait for transcription to complete
    And I send toggle command to stop recording
    Then the output should contain "Transcript 1: hello"

  @at05
  Scenario: JSONL output formatting
    Given a mocked audio backend with sample audio
    And a mocked transcriber returning "hello"
    And the output format is set to "jsonl"
    When I send toggle command to start recording
    And I wait for transcription to complete
    And I send toggle command to stop recording
    Then the output should contain valid JSONL with type "final"
    And the output should contain text "hello"
    And the output should contain timestamp
    And the output should contain utterance
    And the output should contain duration_ms

  @at06
  Scenario: Offline operation
    Given the config directory is empty
    And download_model is disabled
    When I start the daemon in offline mode
    Then the daemon should start successfully
    And the daemon should operate without network access

  @at07
  Scenario: GPU auto-select
    Given the config directory is empty
    And download_model is disabled
    When I start the daemon
    Then the daemon should select GPU backend automatically

  @at07
  Scenario: CPU fallback
    Given the config directory is empty
    And download_model is disabled
    And GPU is not available
    When I start the daemon
    Then the daemon should fallback to CPU backend
    And the logs should contain "using CPU"
