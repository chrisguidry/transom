package sh.transom.notifications

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.drawable.Icon
import java.io.ByteArrayOutputStream
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode
import sh.transom.R
import sh.transom.exchange.png
import sh.transom.pictures.glyph
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.portray
import sh.transom.testserver.send

/** The icon @bob's status bar shows for a message from @alice. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class StatusBarIconTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    private lateinit var alice: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        connected.signedIn(server, "bob")
    }

    private fun smallIcon(): Icon =
        awaitPosted(server.send(alice, "bob", """{"summary": "Updated sift"}""")).smallIcon

    @Test
    fun aSenderWithAMarkOnAFlatPictureShowsItsMark() {
        server.portray(alice, "alice", "Flux", encoded(glyph(Color.BLUE, Color.WHITE)))
        assertEquals(Icon.TYPE_BITMAP, smallIcon().type)
    }

    @Test
    fun aSenderWithAPhotoShowsTransom() {
        server.portray(alice, "alice", "Alice", png(8, 8))
        assertEquals(R.drawable.ic_notification, smallIcon().resId)
    }

    @Test
    fun aSenderWithNoPictureShowsTransom() =
        assertEquals(R.drawable.ic_notification, smallIcon().resId)
}

private fun encoded(picture: Bitmap): ByteArray =
    ByteArrayOutputStream()
        .also { picture.compress(Bitmap.CompressFormat.PNG, 100, it) }
        .toByteArray()
