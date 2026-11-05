package sh.transom.pictures

import java.io.File
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.GraphicsMode
import sh.transom.exchange.png
import sh.transom.sync.Client
import sh.transom.testserver.TestServer
import sh.transom.testserver.call
import sh.transom.testserver.field
import sh.transom.testserver.portray

/** Pictures fetched with the session from a real server, and kept on disk by URL. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class PicturesTest {
    @get:Rule val server = TestServer()
    @get:Rule val folder = TemporaryFolder()

    private lateinit var alice: String
    private lateinit var url: String
    private lateinit var pictures: Pictures

    @Before
    fun portrayed() {
        alice = server.tokenFor("alice")
        server.portray(alice, "alice", "Alice", png(512, 512))
        url = field(server.call("GET", "/@alice", alice), "picture")
        pictures =
            Pictures(Client(server.url, server.tokenFor("bob")), File(folder.root, "pictures"))
    }

    @Test
    fun decodesAtTheSizeAskedForOrLarger() = runBlocking {
        assertEquals(128, pictures.bitmap(url, 100)?.width)
    }

    @Test
    fun keepsAPictureOnceFetched() = runBlocking {
        pictures.bitmap(url, 64)
        server.call("DELETE", "/@alice/picture", alice, expected = 204)
        assertEquals(512, pictures.bitmap(url, 512)?.width)
    }

    @Test
    fun hasNoneForAPictureTheServerDoesNotHave() = runBlocking {
        server.call("DELETE", "/@alice/picture", alice, expected = 204)
        assertNull(pictures.bitmap(url, 64))
    }

    @Test fun hasNoneWithoutAUrl() = runBlocking { assertNull(pictures.bitmap(null, 64)) }
}
