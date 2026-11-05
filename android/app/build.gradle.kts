import kotlinx.kover.gradle.plugin.dsl.CoverageUnit
import org.gradle.api.tasks.testing.logging.TestExceptionFormat

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
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

// The tests that talk to a server run a real transom and test-issuer, which
// cargo builds from the workspace beside this project. Gradle reruns cargo
// only when a file of either crate or of the workspace changes, and cargo
// then decides what to rebuild. The target directory is named, so the
// binaries are where the tests look whatever CARGO_TARGET_DIR says.
val workspace = rootProject.layout.projectDirectory.dir("..")
val cargoTarget = workspace.dir("target")
val transomBinary = cargoTarget.file("debug/transom")
val testIssuerBinary = cargoTarget.file("debug/test-issuer")
val buildServer =
    tasks.register<Exec>("buildServer") {
        description = "Builds the transom and test-issuer binaries that the tests run."
        workingDir(workspace)
        commandLine(
            "cargo",
            "build",
            "--locked",
            "--bin",
            "transom",
            "--bin",
            "test-issuer",
            "--target-dir",
            cargoTarget.asFile.absolutePath,
        )
        // Exec takes the environment when the build is configured, and the
        // configuration cache keeps it, so the variable that limits cargo's
        // parallel jobs is read as an input of the configuration instead.
        providers.environmentVariable("CARGO_BUILD_JOBS").orNull?.let {
            environment("CARGO_BUILD_JOBS", it)
        }
        inputs
            .files(workspace.files("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"))
            .withPropertyName("workspace")
            .withPathSensitivity(PathSensitivity.RELATIVE)
        inputs
            .dir(workspace.dir("server"))
            .withPropertyName("server")
            .withPathSensitivity(PathSensitivity.RELATIVE)
        inputs
            .dir(workspace.dir("test-issuer"))
            .withPropertyName("testIssuer")
            .withPathSensitivity(PathSensitivity.RELATIVE)
        outputs.files(transomBinary, testIssuerBinary).withPropertyName("binaries")
    }

// The Redis the test servers keep their keys in, as the server's own tests
// find it.
val testRedisUrl =
    providers.environmentVariable("TRANSOM_TEST_REDIS_URL").orElse("redis://127.0.0.1:6379")

tasks.withType<Test>().configureEach {
    dependsOn(syncRobolectricSdk)
    inputs.dir(robolectricSdkDir)
    inputs.file(transomCss)
    inputs
        .files(buildServer)
        .withPropertyName("serverBinaries")
        .withPathSensitivity(PathSensitivity.NONE)
    systemProperty("transom.binary", transomBinary.asFile.absolutePath)
    systemProperty("test-issuer.binary", testIssuerBinary.asFile.absolutePath)
    systemProperty("transom.test.redis.url", testRedisUrl.get())
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
    reports {
        // Every line counts, Compose's generated classes included, since the
        // tests run them all. The one exception is code marked
        // @AndroidKeystoreOnly: Robolectric has no AndroidKeyStore provider,
        // so only a device runs it. It is kept to the calls that reach the
        // Keystore, and the encryption around them runs in the tests with a
        // key of the test's own.
        filters { excludes { annotatedBy("sh.transom.signin.AndroidKeystoreOnly") } }
        variant("app") { verify { rule { minBound(100, CoverageUnit.LINE) } } }
    }
}

tasks.named("check") { dependsOn("koverVerifyApp") }

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.activity.compose)
    implementation(libs.browser)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.okhttp)

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
