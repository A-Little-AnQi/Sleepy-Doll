pluginManagement {
    repositories { google(); mavenCentral(); gradlePluginPortal() }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories { google(); mavenCentral() }
}
rootProject.name = "SleepyDollAndroid"
include(":ui", ":app", ":preview-host")
project(":ui").projectDir = file("../packages/ui-android")
