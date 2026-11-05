// Google's Maven Central mirror is listed first: repo.maven.apache.org rate-limits
// shared CI runners with HTTP 429 often enough to break builds, and the mirror
// serves the same artefacts without that throttling.
pluginManagement {
    repositories {
        google()
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositories {
        google()
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        mavenCentral()
    }
}

rootProject.name = "social-mail-android"
include(":app")
