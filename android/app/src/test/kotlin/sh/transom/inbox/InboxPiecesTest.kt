package sh.transom.inbox

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performCustomAccessibilityActionWithLabel
import java.io.File
import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import sh.transom.exchange.Choice
import sh.transom.exchange.Disposition
import sh.transom.exchange.Exchange
import sh.transom.exchange.Message
import sh.transom.exchange.State
import sh.transom.pictures.Pictures
import sh.transom.signin.app
import sh.transom.sync.Client
import sh.transom.sync.Inbox
import sh.transom.sync.InboxEntry
import sh.transom.testserver.NOWHERE
import sh.transom.theme.TransomTheme

/** @bob's inbox drawn from a copy made here, with what each tap and swipe asks for. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@OptIn(ExperimentalTestApi::class)
class InboxPiecesTest {
    @get:Rule val compose = createComposeRule()

    private val now = Instant.parse("2026-11-05T12:00:00Z")
    private val asked = mutableListOf<String>()

    private val actions =
        InboxActions(
            onOpen = { asked += "open $it" },
            onRead = { asked += "read $it" },
            onUnread = { asked += "unread $it" },
            onAnswer = { id, key -> asked += "answer $id $key" },
        )

    private fun entry(
        id: String,
        minutes: Long = 1,
        read: Boolean = false,
        disposition: Disposition? = null,
        choices: List<Choice> = emptyList(),
    ) =
        InboxEntry(
            Exchange(
                id = id,
                url = "/exchanges/$id",
                state = State.OPEN,
                question = choices.isNotEmpty(),
                from = "@alertmanager",
                to = "@bob",
                created = now.minusSeconds(minutes * 60),
                message = Message(summary = id, disposition = disposition, choices = choices),
            ),
            read,
            null,
        )

    private fun show(vararg entries: InboxEntry) {
        val pictures = Pictures(Client(NOWHERE, "token"), File(app.cacheDir, "pictures"))
        compose.setContent {
            TransomTheme {
                InboxScreen("bob", Inbox("bob", entries.toList(), now), pictures, actions)
            }
        }
    }

    @Test
    fun eachSectionSaysHowManyItHolds() {
        show(
            entry("asked", choices = listOf(Choice("Yes"))),
            entry("Disk full"),
            entry("Backup done"),
        )
        listOf("Waiting on you, 1", "Unread, 2").forEach {
            compose.onNode(hasContentDescription(it)).assertExists()
        }
    }

    @Test
    fun nothingToDoSaysSo() {
        show(entry("Backup done", read = true))
        compose.onNodeWithText("You're all caught up.").assertExists()
    }

    @Test
    fun olderRowsWaitBehindShowOlder() {
        show(entry("Last month", minutes = 30L * 24 * 60, read = true))
        compose.onNodeWithText("Show 1 older").performClick()
        compose.onNodeWithText("Last month").assertExists()
    }

    @Test
    fun aRowAnswersWithAChoiceInOneTap() {
        show(entry("Restart?", choices = listOf(Choice("Raise and restart", id = "raise"))))
        compose.onNodeWithText("Raise and restart").performClick()
        assertEquals(listOf("answer Restart? raise"), asked)
    }

    @Test
    fun markingARowReadOffersUndo() {
        show(entry("Disk full"))
        compose.onNode(hasText("Disk full")).performCustomAccessibilityActionWithLabel("Mark read")
        compose.waitUntil {
            compose.onAllNodes(hasText("Marked read")).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("Undo").performClick()
        compose.waitUntil { "unread [Disk full]" in asked }
        assertEquals(listOf("read [Disk full]", "unread [Disk full]"), asked)
    }

    @Test
    fun markAllReadSaysHowManyItMarked() {
        show(
            entry("Disk full"),
            entry("Backup done"),
        )
        compose.onNodeWithText("Mark all read").performClick()
        compose.waitUntil {
            compose.onAllNodes(hasText("Marked 2 read")).fetchSemanticsNodes().isNotEmpty()
        }
        assertEquals(listOf("read [Disk full, Backup done]"), asked)
    }

    @Test
    fun aRowCarriesTheBadgeOfItsDisposition() {
        show(
            entry("Disk full", disposition = Disposition.PROBLEM),
            entry("Restart?", choices = listOf(Choice("Yes"))),
            entry("Backup done", disposition = Disposition.INFORMATIONAL),
        )
        assertEquals(listOf(1, 0, 1), badges())
    }

    @Test
    fun aReadRowKeepsItsBadge() {
        show(entry("Disk fine", read = true, disposition = Disposition.RESOLVED))
        assertEquals(listOf(0, 1, 0), badges())
    }

    /** How many rows carry the badge of a problem, of a resolution, and of a question. */
    private fun badges(): List<Int> =
        listOf("Problem", "Resolved", "Question").map {
            compose.onAllNodes(hasContentDescription(it)).fetchSemanticsNodes().size
        }

    @Test
    @Config(qualifiers = "night")
    fun drawsInTheDark() {
        show(
            entry("Disk full", disposition = Disposition.PROBLEM),
            entry("Backup done", read = true, disposition = Disposition.RESOLVED),
        )
        compose.onNodeWithText("Backup done").performClick()
        assertEquals(listOf("open Backup done"), asked)
    }
}
