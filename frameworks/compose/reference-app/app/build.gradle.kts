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
    )
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
}


dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.13.0")
}
