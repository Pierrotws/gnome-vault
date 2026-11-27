plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// The Rust library (libpassel_ffi.so per ABI) and its UniFFI Kotlin
// bindings, built by ../build-rust.sh before every build. Cargo decides
// what actually needs rebuilding.
val rustOut = layout.buildDirectory.dir("generated/rust")
val rustAbis = providers.gradleProperty("passel.abis").get().split(" ").filter(String::isNotBlank)

val passelVersion = providers.gradleProperty("passel.version").get()

/** `1.2.3` → 1002003: increases with every release, as Google Play requires. */
fun versionCodeOf(version: String): Int {
    val (major, minor, patch) = version.split(".").map { part -> part.takeWhile(Char::isDigit).toInt() }
    return major * 1_000_000 + minor * 1_000 + patch
}

val buildRust = tasks.register<Exec>("buildRust") {
    description = "Builds libpassel_ffi.so and its Kotlin bindings"
    workingDir = rootDir
    commandLine(listOf("bash", rootDir.resolve("build-rust.sh").path, rustOut.get().asFile.path) + rustAbis)
    outputs.dir(rustOut)
    outputs.upToDateWhen { false }
}

android {
    namespace = "io.pierrotws.passel"
    compileSdk {
        version = release(37) { minorApiLevel = 1 }
    }

    defaultConfig {
        applicationId = "io.pierrotws.passel"
        minSdk = 26
        targetSdk = 36
        // Passel's version (release-please bumps it in gradle.properties);
        // the code follows it, so every release's code is higher.
        versionName = passelVersion
        versionCode = versionCodeOf(passelVersion)
        ndk { abiFilters += rustAbis }
    }

    // The Play upload key, given by CI (or by hand for the first upload):
    // without it, release builds are left unsigned.
    val uploadKeystore = providers.environmentVariable("PASSEL_UPLOAD_KEYSTORE").orNull
    signingConfigs {
        if (uploadKeystore != null) {
            create("upload") {
                storeFile = file(uploadKeystore)
                storePassword = providers.environmentVariable("PASSEL_UPLOAD_KEYSTORE_PASSWORD").get()
                keyAlias = providers.environmentVariable("PASSEL_UPLOAD_KEY_ALIAS").get()
                keyPassword = providers.environmentVariable("PASSEL_UPLOAD_KEY_PASSWORD").get()
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("upload")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        getByName("main") {
            jniLibs.directories += rustOut.get().dir("jniLibs").asFile.path
            kotlin.directories += rustOut.get().dir("kotlin").asFile.path
        }
    }
}

tasks.named("preBuild") { dependsOn(buildRust) }

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    // Auto-lock: knows when the whole app goes to the background.
    implementation(libs.lifecycle.process)
    implementation(libs.biometric)
    // Google Drive: Google sign-in and consent for the drive.file scope.
    implementation(libs.play.services.auth)
    // UniFFI's Kotlin bindings call the Rust library through JNA.
    implementation(libs.jna) { artifact { type = "aar" } }
}
