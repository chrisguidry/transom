package sh.transom.signin

import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/** What a person types for the server becomes a URL the app can add its paths to. */
@RunWith(Parameterized::class)
class ServerUrlTest(private val typed: String, private val expected: String?) {
    @Test
    fun normalizes() {
        assertEquals(expected, serverUrl(typed))
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0} -> {1}")
        fun cases(): List<Array<Any?>> =
            listOf(
                arrayOf("transom.example.com", "https://transom.example.com"),
                arrayOf("  transom.example.com/  ", "https://transom.example.com"),
                arrayOf("https://transom.example.com/", "https://transom.example.com"),
                arrayOf("Transom.Example.com", "https://transom.example.com"),
                arrayOf("http://127.0.0.1:8080", "http://127.0.0.1:8080"),
                arrayOf("example.com/transom/", "https://example.com/transom"),
                arrayOf("transom.example.com/?next=/#top", "https://transom.example.com"),
                arrayOf("", null),
                arrayOf("   ", null),
                arrayOf("ftp://transom.example.com", null),
                arrayOf("https://", null),
            )
    }
}
