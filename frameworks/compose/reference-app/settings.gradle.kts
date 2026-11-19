// Real-Android build of examples/compose-* for reference screenshots (tests/fixtures/compose-android).
pluginManagement { repositories { google(); mavenCentral(); gradlePluginPortal() } }
dependencyResolutionManagement { repositories { google(); mavenCentral() } }
rootProject.name = "onesim-compose-reference"
include(":app")
