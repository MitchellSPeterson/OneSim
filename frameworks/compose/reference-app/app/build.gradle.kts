plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.simcore.reference"
    compileSdk = 37
    defaultConfig {
        applicationId = "dev.simcore.reference"
        minSdk = 26
        targetSdk = 36
    }
    // Compile the OneSim examples unmodified.
    sourceSets["main"].kotlin.directories += listOf(
        "../../../../examples/compose-counter",
        "../../../../examples/compose-showcase",
        "../../../../examples/compose-input",
        "../../../../examples/compose-interaction",
        "../../../../examples/compose-navigation",
    )
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
}


dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    // The API inventory's material3 (docs/compatibility/versions.json): the flexible app bars are public from 1.5.
    implementation("androidx.compose.material3:material3:1.5.0-beta01")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.navigation:navigation-compose:2.10.0-rc01")
}
