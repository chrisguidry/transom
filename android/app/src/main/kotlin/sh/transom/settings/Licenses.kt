package sh.transom.settings

/**
 * A library the app ships, or a family of them released together: its [name], the Maven [groups]
 * its modules are published under, where `androidx.*` stands for every group under `androidx`, and
 * its [license], whose text is at [url].
 */
data class Library(
    val name: String,
    val groups: List<String>,
    val license: String,
    val url: String,
)

/**
 * Every library in the release build, kept by hand. LicensesTest compares it with the modules the
 * release build resolves, so a dependency added or removed without a change here fails the build.
 */
val LIBRARIES =
    listOf(
        Library("AndroidX and Jetpack Compose", listOf("androidx.*"), APACHE, APACHE_URL),
        Library(
            "commonmark-java",
            listOf("org.commonmark"),
            "BSD 2-Clause License",
            "https://github.com/commonmark/commonmark-java/blob/main/LICENSE.txt",
        ),
        Library("Guava's ListenableFuture", listOf("com.google.guava"), APACHE, APACHE_URL),
        Library("JetBrains Java Annotations", listOf("org.jetbrains"), APACHE, APACHE_URL),
        Library("JSpecify", listOf("org.jspecify"), APACHE, APACHE_URL),
        Library("Kotlin", listOf("org.jetbrains.kotlin"), APACHE, APACHE_URL),
        Library(
            "kotlinx.coroutines and kotlinx.serialization",
            listOf("org.jetbrains.kotlinx"),
            APACHE,
            APACHE_URL,
        ),
        Library(
            "OkHttp and Okio",
            listOf("com.squareup.okhttp3", "com.squareup.okio"),
            APACHE,
            APACHE_URL,
        ),
    )

private const val APACHE = "Apache License 2.0"
private const val APACHE_URL = "https://www.apache.org/licenses/LICENSE-2.0"
