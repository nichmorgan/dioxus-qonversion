pluginManagement {
    repositories { google(); mavenCentral(); gradlePluginPortal() }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories { google(); mavenCentral() }
}
rootProject.name = "NativeRemoteConfigTest"
include(":app", ":qonversion-host")
// Compile the existing host and its existing SDK dependency, not a copy.
project(":qonversion-host").projectDir = file("../../../android")
