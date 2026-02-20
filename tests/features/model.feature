Feature: Model management
  @at01a
  Scenario: Missing model is auto-downloaded
    Given no model file exists
    And the model base URL is available
    When I request a model download
    Then the model should be downloaded automatically
    And the downloaded model should be stored in the models directory

  @at01b
  Scenario: Language selects model variant
    Given language is set to "en"
    When I request an English model
    Then the model filename should contain ".en."
    Given language is set to "auto"
    When I request an auto-detected model
    Then the model filename should not contain ".en."
