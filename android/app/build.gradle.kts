plugins {
    id("com.android.application")
}

android {
    namespace = "com.fidderr.alarmfree.slint"
    compileSdk = 36
    defaultConfig {
        applicationId = "com.fidderr.alarmfree.slint"
        minSdk = 31
        targetSdk = 36
        versionCode = 1
        versionName = "0.4.1"
    }
    buildTypes {
        getByName("debug") {
            isDebuggable = true
            isMinifyEnabled = false
        }
        getByName("release") {
            isMinifyEnabled = false
            // A copy for comparing, never for Play: the debug key keeps it installable.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    // The Java source stays in src/java; build.sh stages the native libraries into build/jniLibs
    // and the manifest and theme into app/src/main, where build_sentinel adds the SDK's parts.
    sourceSets {
        getByName("main") {
            java.srcDirs("../src/java")
            jniLibs.srcDirs("../../build/jniLibs")
        }
    }
}

dependencies {
}
