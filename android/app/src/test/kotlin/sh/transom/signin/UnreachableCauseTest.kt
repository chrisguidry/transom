package sh.transom.signin

import java.io.EOFException
import java.io.IOException
import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import java.security.cert.CertificateException
import javax.net.ssl.SSLHandshakeException
import javax.net.ssl.SSLPeerUnverifiedException
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized
import sh.transom.R

/** The plain words under "That server can't be reached" for the failures people meet most. */
@RunWith(Parameterized::class)
class UnreachableCauseTest(
    @Suppress("unused") private val name: String,
    private val cause: IOException,
    private val words: Int?,
) {
    @Test
    fun saysWhyInPlainWords() {
        assertEquals(words, classified(cause))
    }

    companion object {
        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf(
                    "no address",
                    UnknownHostException("x.invalid"),
                    R.string.sign_in_no_address,
                ),
                arrayOf("timed out", SocketTimeoutException("timeout"), R.string.sign_in_timed_out),
                arrayOf("untrusted", untrusted(), R.string.sign_in_untrusted),
                arrayOf("another handshake failure", SSLHandshakeException("handshake"), null),
                arrayOf(
                    "another name",
                    SSLPeerUnverifiedException("Hostname x not verified"),
                    R.string.sign_in_another_name,
                ),
                arrayOf("refused", ConnectException("Failed to connect"), null),
            )

        private fun untrusted() =
            SSLHandshakeException("handshake").apply { initCause(CertificateException()) }
    }
}

/** The messages shown for a failure that has no plain words of its own. */
class UnreachableDescribedTest {
    @Test
    fun givesEachMessageInTheChainOfCauses() {
        val cause =
            ConnectException("Failed to connect to /10.0.0.240:443").apply {
                initCause(ConnectException("connect failed: EPERM (Operation not permitted)"))
            }
        assertEquals(
            "Failed to connect to /10.0.0.240:443: connect failed: EPERM (Operation not permitted)",
            described(cause),
        )
    }

    @Test
    fun namesAFailureThatHasNoMessage() {
        assertEquals("EOFException", described(EOFException()))
    }
}
