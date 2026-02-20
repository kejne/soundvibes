Feature: Marketing website
  @at10
  Scenario: Marketing site builds and smoke test
    Given npm is available
    And the web directory exists
    When I run npm install
    Then the installation should succeed
    When I run npm run build
    Then the build should succeed
    When I run npm run test:ui
    Then the smoke tests should pass
