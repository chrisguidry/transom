import kotlinx.kover.gradle.plugin.dsl.CoverageUnit
import org.gradle.api.tasks.testing.logging.TestExceptionFormat

/** Each Gradle property `transom.firebase.*`, and the BuildConfig field it becomes. */
val firebaseValues =
    mapOf(
        "projectId" to "FIREBASE_PROJECT_ID",
        "applicationId" to "FIREBASE_APPLICATION_ID",
        "apiKey" to "FIREBASE_API_KEY",
        "senderId" to "FIREBASE_SENDER_ID",
    )

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.kover)
    alias(libs.plugins.ksp)
    alias(libs.plugins.ktfmt)
    alias(libs.plugins.room)
}

android {
    namespace = "sh.transom"
    compileSdk = 37

    defaultConfig {
        applicationId = "sh.transom"
        minSdk = 29
        targetSdk = 37
        // A release's version comes from the workflow that publishes it: the name is the
        // release's tag, and the code is the build's time in whole minutes since the Unix epoch, so
        // each release's code is higher than the one before and Android installs it as an
        // update. A build without them, such as a local one, is version 1, named dev.
        versionCode =
            providers.gradleProperty("transom.versionCode").map(String::toInt).getOrElse(1)
        versionName = providers.gradleProperty("transom.versionName").getOrElse("dev")
        // The end-to-end test in src/androidTest, which end-to-end.sh runs on
        // an emulator with connectedDebugAndroidTest. That task needs a
        // device, so it is not part of check.
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // The Firebase project's values, which gateway/firebase.tf outputs, from Gradle
        // properties: -Ptransom.firebase.projectId=… on the command line, or the same names in
        // ~/.gradle/gradle.properties. The app reads them into FirebaseOptions itself, so the
        // build needs no google-services.json and no Google Services plugin. None of them is
        // secret. A build without all four has no FCM, and the phone holds its connection to the
        // server instead. Each value is written into Java source as it is, so one with a
        // character that Firebase never uses, such as a quote, fails the build.
        for ((property, field) in firebaseValues) {
            val value = providers.gradleProperty("transom.firebase.$property").getOrElse("")
            require(value.all { it.isLetterOrDigit() || it in ":-_." }) {
                "transom.firebase.$property has a character no Firebase value has: $value"
            }
            buildConfigField("String", field, "\"$value\"")
        }
    }

    // The release key, from a PKCS12 keystore whose path and password the environment names, so
    // the password never appears on a command line. The store and the key share the password.
    // The APK carries signature schemes v2 and v3. Android checks v1, the JAR signature, only below
    // API 24, and minSdk is above it. The Android Gradle Plugin leaves v3 off unless it is asked
    // for, and v3 is the scheme that lets a later release rotate to a new key. A build without
    // TRANSOM_RELEASE_KEYSTORE_FILE has no release signing config, and its release APK is
    // unsigned; every other task, check among them, runs the same either way.
    val releaseKeystore = providers.environmentVariable("TRANSOM_RELEASE_KEYSTORE_FILE")
    if (releaseKeystore.isPresent) {
        val password = providers.environmentVariable("TRANSOM_RELEASE_KEYSTORE_PASSWORD")
        signingConfigs.create("release") {
            storeFile = file(releaseKeystore.get())
            storeType = "pkcs12"
            storePassword = password.get()
            keyAlias = "transom"
            keyPassword = password.get()
            enableV1Signing = false
            enableV2Signing = true
            enableV3Signing = true
        }
        buildTypes.named("release") { signingConfig = signingConfigs.getByName("release") }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        buildConfig = true
        compose = true
    }

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

// Room writes each version of the database's schema here, so a later
// version's migration can be written and tested against the one before.
room { schemaDirectory("$projectDir/schemas") }

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
    // The licenses test compares the app's hand-kept list of libraries with
    // every module the release build ships, read from its resolved graph.
    val shipped =
        configurations
            .named("releaseRuntimeClasspath")
            .flatMap { it.incoming.resolutionResult.rootComponent }
            .map { root ->
                val modules = sortedSetOf<String>()
                val seen = mutableSetOf(root)
                val next = ArrayDeque(listOf(root))
                while (next.isNotEmpty()) {
                    for (dependency in next.removeFirst().dependencies) {
                        val selected = (dependency as? ResolvedDependencyResult)?.selected
                        if (selected != null && seen.add(selected)) {
                            next.add(selected)
                            selected.moduleVersion?.let { modules.add("${it.group}:${it.name}") }
                        }
                    }
                }
                modules.joinToString(",")
            }
    inputs.property("shippedModules", shipped)
    jvmArgumentProviders.add(
        CommandLineArgumentProvider { listOf("-Dtransom.shipped=${shipped.get()}") }
    )
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
        // tests run them all, with three exceptions. Code marked
        // @AndroidKeystoreOnly: Robolectric has no AndroidKeyStore provider,
        // so only a device runs it. It is kept to the calls that reach the
        // Keystore, and the encryption around them runs in the tests with a
        // key of the test's own. Code marked @FirebaseOnly: Robolectric has
        // no Google Play services, and the tests' build has no Firebase
        // project, so only a phone with both starts Firebase and asks it for
        // a token. It is kept to the calls that reach Firebase and Play
        // services, and the tests give the app a token of their own in its
        // place, through the same Push the app uses. And what Room generates for the database,
        // its DAOs, and its automatic migrations, the classes named *_Impl.
        // A migration runs only on a phone that has the version before, which
        // a test's fresh database never is, and it is made from the schemas in
        // schemas/; the emulator ran the one to version 2. Room also writes
        // code there for cases the app never reaches: checking the schema
        // during a migration, clearing every table, a branch for null in each column that can
        // hold one whether or not a query ever reads one, and an error for a
        // query that requires a row it cannot miss. The tests run every
        // query the DAOs declare, through the DAOs. PhoneCamera, which
        // binds CameraX to the phone's camera: Robolectric has no camera, so
        // the emulator runs it. The tests give the scanner a camera of their
        // own, and read codes through qrText, which PhoneCamera calls. And
        // BuildConfig, which the Android Gradle Plugin writes, holds only
        // constants, which the compiler puts in place where they are read,
        // and a constructor nothing calls.
        filters {
            excludes {
                annotatedBy("sh.transom.signin.AndroidKeystoreOnly")
                annotatedBy("sh.transom.delivery.FirebaseOnly")
                classes(
                    "sh.transom.signin.PhoneCamera",
                    "sh.transom.signin.PhoneCamera$*",
                    "sh.transom.BuildConfig",
                    "sh.transom.sync.*_Impl",
                    "sh.transom.sync.*_Impl$*",
                    "sh.transom.notifications.*_Impl",
                    "sh.transom.notifications.*_Impl$*",
                )
            }
        }
        variant("app") { verify { rule { minBound(100, CoverageUnit.LINE) } } }
    }
}

tasks.named("check") { dependsOn("koverVerifyApp") }

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.activity.compose)
    implementation(libs.browser)
    implementation(libs.camera.camera2)
    implementation(libs.camera.compose)
    implementation(libs.camera.lifecycle)
    implementation(libs.commonmark)
    implementation(libs.commonmark.strikethrough)
    implementation(libs.commonmark.tables)
    implementation(libs.commonmark.task.lists)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui)
    implementation(libs.firebase.messaging)
    implementation(libs.fragment)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.okhttp)
    implementation(libs.okhttp.coroutines)
    implementation(libs.okhttp.sse)
    implementation(libs.play.services.base)
    implementation(libs.room.runtime)
    implementation(libs.work.runtime)
    implementation(libs.zxing.core)
    ksp(libs.room.compiler)

    testImplementation(platform(libs.compose.bom))
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.espresso.core)
    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    testImplementation(libs.work.testing)
    debugImplementation(libs.compose.ui.test.manifest)

    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.uiautomator)

    ktfmt(libs.ktfmt)

    robolectricSdk(libs.robolectric.android.all)
}
