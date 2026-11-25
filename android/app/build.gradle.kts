plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// The Rust library (libpassel_ffi.so per ABI) and its UniFFI Kotlin
// bindings, built by ../build-rust.sh before every build. Cargo decides
// what actually needs rebuilding.
val rustOut = layout.buildDirectory.dir("generated/rust")
val rustAbis = providers.gradleProperty("passel.abis").get().split(" ").filter(String::isNotBlank)

val buildRust by tasks.registering(Exec::class) {
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
        versionCode = 1
        versionName = "0.1.0"
        ndk { abiFilters += rustAbis }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
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
    // UniFFI's Kotlin bindings call the Rust library through JNA.
    implementation(libs.jna) { artifact { type = "aar" } }
}
