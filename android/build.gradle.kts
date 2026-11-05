plugins {
    base
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kover) apply false
    alias(libs.plugins.ktfmt)
}

// The kotlinlang style follows Kotlin's own coding conventions, with
// four-space indents, so it agrees with Android Studio's formatter under
// kotlin.code.style=official and with the server's rustfmt. Each project
// that applies ktfmt sets the same style.
ktfmt { kotlinLangStyle() }

// The app's check covers its own sources and build script, and this covers
// the scripts beside this one.
tasks.check { dependsOn(tasks.named("ktfmtCheckScripts")) }
