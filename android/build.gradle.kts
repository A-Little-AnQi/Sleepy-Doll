buildscript {
    repositories { google(); mavenCentral() }
    dependencies { classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.21") }
}
plugins {
    id("com.android.application") version "9.4.1" apply false
    id("com.android.library") version "9.4.1" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.4.21" apply false
}
allprojects {
    group = "dev.sleepydoll"
    version = "0.1.0"
    layout.buildDirectory.set(rootProject.layout.projectDirectory.dir("../target/android/build/" + project.name))
}
