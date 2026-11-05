plugins {
    id("com.android.application")
    kotlin("android")
}
android {
    namespace = "io.dioxus.qonversion.nativetest"
    compileSdk = 34
    System.getenv("QONVERSION_TEST_RAW_DIR")?.let { raw ->
        sourceSets.getByName("main").jniLibs.setSrcDirs(listOf("$raw/jniLibs"))
    }
    defaultConfig {
        applicationId = "io.dioxus.qonversion.nativetest"
        minSdk = 24
        targetSdk = 34
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        testInstrumentationRunnerArguments["runNativeRemoteConfig"] =
            providers.gradleProperty("runNativeRemoteConfig").orElse("false").get()
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}
dependencies {
    implementation(project(":qonversion-host"))
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
}
