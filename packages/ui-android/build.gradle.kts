import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import groovy.json.JsonSlurper
import org.gradle.api.file.DirectoryProperty
import org.gradle.api.file.RegularFileProperty
import org.gradle.api.tasks.InputFile
import org.gradle.api.tasks.OutputDirectory
import org.gradle.api.tasks.TaskAction

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.plugin.compose")
    id("maven-publish")
}
abstract class GenerateSleepyTheme : DefaultTask() {
    @get:InputFile abstract val themeInput: RegularFileProperty
    @get:OutputDirectory abstract val themeOutput: DirectoryProperty
    @TaskAction fun generate() {
        val input = JsonSlurper().parse(themeInput.get().asFile) as Map<*, *>
        val source = StringBuilder("package dev.sleepydoll.ui\n\ninternal object SleepyPalette {\n")
        for (mode in listOf("light", "dark")) {
            val values = input[mode] as Map<*, *>
            for ((key, value) in values) {
                val color = value.toString()
                require(Regex("#[0-9a-fA-F]{6}").matches(color))
                source.append("    const val ").append(mode).append(key.toString().replaceFirstChar { it.uppercase() }).append(" = 0xFF").append(color.drop(1)).append("\n")
            }
        }
        val design = input["design"] as Map<*, *>
        for (key in listOf("radius", "fontSize", "borderWidth")) source.append("    const val $key = ${design[key]}f\n")
        source.append("    const val motionMillis = ${design["motionMillis"]}\n    const val shadow = ${design["shadow"]}\n}\n")
        val directory = themeOutput.get().asFile
        directory.mkdirs()
        directory.resolve("SleepyPalette.kt").writeText(source.toString())
    }
}
val generateSleepyTheme = tasks.register<GenerateSleepyTheme>("generateSleepyTheme") {
    themeInput.set(layout.projectDirectory.file("theme.json"))
    themeOutput.set(layout.buildDirectory.dir("generated/sleepy-theme"))
}
android {
    namespace = "dev.sleepydoll.ui"
    compileSdk = 37
    defaultConfig { minSdk = 26 }
    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    publishing { singleVariant("release") { withSourcesJar() } }
}
androidComponents.onVariants { variant ->
    variant.sources.kotlin?.addGeneratedSourceDirectory(generateSleepyTheme, GenerateSleepyTheme::themeOutput)
}
kotlin { compilerOptions { jvmTarget.set(JvmTarget.JVM_17) } }
dependencies {
    api(platform(libs.compose.bom))
    api(libs.compose.ui)
    api(libs.compose.foundation)
    api(libs.compose.material3)
    implementation(libs.compose.tooling.preview)
    debugImplementation(libs.compose.tooling)
}
publishing {
    repositories {
        maven { name = "Project"; url = uri(rootProject.file("../target/android/maven")) }
    }
}
afterEvaluate {
    publishing.publications.create<MavenPublication>("release") {
        from(components["release"])
        artifactId = "ui"
        pom {
            name.set("Sleepy Doll Android UI")
            description.set("Sleepy Doll 原生 Compose 组件与主题")
        }
    }
}
