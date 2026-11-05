package sh.transom.notifications

import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import sh.transom.online
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.group
import sh.transom.testserver.send

/**
 * @bob's notifications clear even while Android keeps one up after a direct reply and drops the
 * app's cancel of it.
 */
@RunWith(RobolectricTestRunner::class)
@Config(shadows = [ShadowLifetimeExtension::class])
class LifetimeExtensionTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
    }

    /** Alice asks [to], which Bob receives, and Bob's connection is running. */
    private fun asked(to: String = "bob"): String {
        if (to == "bob") server.accept("bob", bob, alice)
        connected.signedIn(server, "bob")
        return server.send(
            alice,
            to,
            """{"summary": "Deploy?", "prompt": "", "choices": ["Deploy", "Hold"]}""",
        )
    }

    @Test
    fun aWithdrawalClearsIt() {
        val id = asked()
        awaitPosted(id)
        ShadowLifetimeExtension.extend(id)
        server.call("DELETE", "/exchanges/$id/message", alice)
        awaitCleared(id)
    }

    @Test
    fun anAnswerTheServerTakesLateClearsIt() {
        val id = asked()
        awaitPosted(id).reply("Hold", chip = true)
        ShadowLifetimeExtension.extend(id)
        awaitPosted(id) { it.actions == null }
        ShadowLifetimeExtension.extend(id)
        await {
            online()
            posted(id) == null
        }
    }

    @Test
    fun anAnswerThatLostClearsOnceRead() {
        server.group("team", alice, "bob" to bob)
        val id = asked(to = "team")
        awaitPosted(id).reply("Deploy", chip = true)
        ShadowLifetimeExtension.extend(id)
        server.call("PUT", "/exchanges/$id/reply", alice, """{"choice": "Hold"}""")
        awaitPosted(id) {
            online()
            it.messages.size == 2 && it.messages.last().startsWith("@alice")
        }
        ShadowLifetimeExtension.extend(id)
        server.call("PUT", "/exchanges/$id/@bob", bob)
        awaitCleared(id)
    }
}
