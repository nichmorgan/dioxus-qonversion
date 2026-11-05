plugins {
    id("com.android.application") version "8.4.2" apply false
    kotlin("android") version "1.9.24" apply false
}

// Isolate all generated APKs, reports and host-module build outputs as raw.
System.getenv("QONVERSION_TEST_RAW_DIR")?.let { raw ->
    allprojects {
        layout.buildDirectory.set(file("$raw/gradle-build/$name"))
    }
}
