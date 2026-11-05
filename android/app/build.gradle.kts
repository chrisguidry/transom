import kotlinx.kover.gradle.plugin.dsl.CoverageUnit
import org.gradle.api.tasks.testing.logging.TestExceptionFormat

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kover)
    alias(libs.plugins.ktfmt)
}

android {
    namespace = "sh.transom"
    compileSdk = 37

    defaultConfig {
        applicationId = "sh.transom"
        minSdk = 29
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures { compose = true }

    testOptions { unitTests.isIncludeAndroidResources = true }

    lint {
        warningsAsErrors = true
        abortOnError = true
        checkDependencies = true
        // These checks compare the build against whatever was released on
        // the day lint runs, so the same commit would pass one day and fail
        // the next. Dependabot proposes each new release instead.
        disable += setOf("AndroidGradlePluginVersion", "GradleDependency", "NewerVersionAvailable")
    }
}

kotlin { compilerOptions { allWarningsAsErrors = true } }

// The same style as the root build file, which says why.
ktfmt { kotlinLangStyle() }

// Robolectric would download the Android framework jar for each SDK it
// runs while the tests run, past Gradle's dependency verification. Gradle
// resolves it here instead, checked against verification-metadata.xml, and
// Robolectric runs offline from the copy.
val robolectricSdk = configurations.register("robolectricSdk")
val robolectricSdkDir = layout.buildDirectory.dir("robolectric-sdk")
val syncRobolectricSdk =
    tasks.register<Sync>("syncRobolectricSdk") {
        from(robolectricSdk)
        into(robolectricSdkDir)
    }

// The palette test reads the web UI's stylesheet, so a change to the
// stylesheet reruns the tests.
val transomCss = rootProject.layout.projectDirectory.file("../server/src/web/static/transom.css")

tasks.withType<Test>().configureEach {
    dependsOn(syncRobolectricSdk)
    inputs.dir(robolectricSdkDir)
    inputs.file(transomCss)
    // One test JVM of bounded size, so the tests fit beside the Gradle
    // daemon on a CI runner.
    maxHeapSize = "1g"
    maxParallelForks = 1
    // Robolectric reaches into the JDK's internals, which JDK 17 and later
    // close unless they are opened, as Robolectric's documentation lists.
    jvmArgs(
        "--add-opens=java.base/java.lang=ALL-UNNAMED",
        "--add-opens=java.base/java.util=ALL-UNNAMED",
        "--add-opens=java.base/java.io=ALL-UNNAMED",
        "--add-opens=java.base/java.net=ALL-UNNAMED",
        "--add-opens=java.base/java.security=ALL-UNNAMED",
        "--add-opens=java.base/java.text=ALL-UNNAMED",
        "--add-opens=java.base/jdk.internal.access=ALL-UNNAMED",
        "--add-opens=java.desktop/java.awt.font=ALL-UNNAMED",
        "--add-opens=jdk.compiler/com.sun.tools.javac.api=ALL-UNNAMED",
    )
    systemProperty("robolectric.offline", "true")
    systemProperty("robolectric.dependency.dir", robolectricSdkDir.get().asFile.absolutePath)
    systemProperty("transom.css", transomCss.asFile.absolutePath)
    // A failure in CI shows its whole assertion, such as which color differs.
    testLogging {
        events("failed")
        exceptionFormat = TestExceptionFormat.FULL
    }
}

kover {
    currentProject {
        // The Android Gradle Plugin runs the unit tests for the debug build
        // only, so the debug build is the one measured.
        createVariant("app") { add("debug") }
    }
    // Every line counts: nothing is excluded, Compose's generated classes
    // included, since the tests run them all.
    reports { variant("app") { verify { rule { minBound(100, CoverageUnit.LINE) } } } }
}

tasks.named("check") { dependsOn("koverVerifyApp") }

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.activity.compose)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui)
    implementation(libs.lifecycle.runtime.compose)

    testImplementation(platform(libs.compose.bom))
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.espresso.core)
    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    debugImplementation(libs.compose.ui.test.manifest)

    ktfmt(libs.ktfmt)

    robolectricSdk(libs.robolectric.android.all)
}
