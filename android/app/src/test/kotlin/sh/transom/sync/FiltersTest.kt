package sh.transom.sync

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import sh.transom.exchange.State
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.send

/**
 * The list narrowed by each filter. Alice is accepted with a "Hello", then asks "Deploy?", sends
 * "Backup finished", which Bob reads, and asks "Restart?", which Bob answers.
 */
@RunWith(ParameterizedRobolectricTestRunner::class)
class FiltersTest(private val filters: Filters, private val summaries: List<String>) {
    @get:Rule val server = TestServer()

    @Test
    fun listsWhatPassesTheFilters() = runBlocking {
        val alice = server.tokenFor("alice")
        val bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        server.send(alice, "bob", """{"summary": "Deploy?", "choices": ["Deploy", "Hold"]}""")
        val backup = server.send(alice, "bob", """{"summary": "Backup finished"}""")
        server.call("PUT", "/exchanges/$backup/@bob", bob)
        val restart = server.send(alice, "bob", """{"summary": "Restart?", "prompt": ""}""")
        server.call("PUT", "/exchanges/$restart/reply", bob, """{"text": "go ahead"}""")
        val listed = Client(server.url, bob).exchanges("bob", filters)
        assertEquals(summaries, listed.filter { it.from == "@alice" }.map { it.message.summary })
    }

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun cases() =
            listOf(
                arrayOf<Any>(Filters(), listOf("Restart?", "Backup finished", "Deploy?", "Hello")),
                arrayOf<Any>(Filters(question = true), listOf("Restart?", "Deploy?")),
                arrayOf<Any>(Filters(question = false), listOf("Backup finished", "Hello")),
                arrayOf<Any>(Filters(read = false), listOf("Deploy?", "Hello")),
                arrayOf<Any>(Filters(read = true), listOf("Restart?", "Backup finished")),
                arrayOf<Any>(Filters(state = State.REPLIED), listOf("Restart?")),
                arrayOf<Any>(Filters(limit = 2), listOf("Restart?", "Backup finished")),
            )
    }
}
