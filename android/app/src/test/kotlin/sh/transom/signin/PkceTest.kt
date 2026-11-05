package sh.transom.signin

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class PkceTest {
    @Test
    fun theChallengeIsRfc7636sExample() {
        // RFC 7636, appendix B.
        assertEquals(
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            challengeOf("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        )
    }

    @Test
    fun aVerifierIs32BytesOfBase64url() {
        assertTrue(Regex("[A-Za-z0-9_-]{43}").matches(newVerifier()))
    }

    @Test
    fun eachVerifierIsNew() {
        assertNotEquals(newVerifier(), newVerifier())
    }
}
