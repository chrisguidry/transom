package sh.transom.notifications

import android.app.Notification
import android.graphics.Bitmap
import android.graphics.drawable.BitmapDrawable
import androidx.core.app.NotificationCompat
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import sh.transom.exchange.message
import sh.transom.exchange.png
import sh.transom.signin.app
import sh.transom.testserver.TestServer
import sh.transom.testserver.accept
import sh.transom.testserver.call
import sh.transom.testserver.portray
import sh.transom.testserver.send

/** What an alert's fields do to @bob's notifications. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(shadows = [ShadowFullScreen::class])
class AlertNotificationsTest {
    @get:Rule val server = TestServer()
    @get:Rule val connected = Connected()

    private lateinit var alice: String
    private lateinit var bob: String

    @Before
    fun people() {
        alice = server.tokenFor("alice")
        bob = server.tokenFor("bob")
        server.accept("bob", bob, alice)
        connected.signedIn(server, "bob")
        ShadowFullScreen.granted = true
    }

    private fun send(json: String) = server.send(alice, "bob", json)

    @Test
    fun anUrgentMessageIsAnAlarmOnTheUrgentChannel() {
        val posted = awaitPosted(send("""{"summary": "UPS on battery", "urgency": "urgent"}"""))
        assertEquals(
            listOf(Channels.URGENT, Notification.CATEGORY_ALARM, false),
            listOf(posted.channelId, posted.category, posted.silent),
        )
    }

    @Test
    fun anUrgentMessageShowsFullScreen() {
        val posted = awaitPosted(send("""{"summary": "UPS on battery", "urgency": "urgent"}"""))
        val page = shadowOf(posted.fullScreenIntent).savedIntent
        assertEquals(AlertActivity::class.java.name, page.component?.className)
    }

    @Test
    fun anUrgentMessageIsAHeadsUpWhereFullScreenIsNotAllowed() {
        ShadowFullScreen.granted = false
        val posted = awaitPosted(send("""{"summary": "UPS on battery", "urgency": "urgent"}"""))
        assertEquals(listOf(null, false), listOf(posted.fullScreenIntent, posted.silent))
    }

    @Test
    fun aHighMessageHasNoFullScreen() =
        assertNull(
            awaitPosted(send("""{"summary": "Disk?", "urgency": "high"}""")).fullScreenIntent
        )

    @Test
    fun aLinksTitleLabelsItsAction() {
        val id =
            send(
                """{"summary": "CI failed", "prompt": "",
                    "link": [{"href": "https://ci.example.com/run/7", "title": "View run"}]}"""
            )
        assertEquals(listOf("Reply", "View run"), awaitPosted(id).actionTitles)
    }

    @Test
    fun theSenderIsShownByDisplayName() {
        server.portray(alice, "alice", "Alertmanager", png(8, 8))
        val id = send("""{"summary": "Postgres is down"}""")
        assertEquals(listOf("Alertmanager: Postgres is down"), awaitPosted(id).messages)
    }

    @Test
    fun theSendersPictureCarriesTheBadgeOfTheMessagesDisposition() {
        server.portray(alice, "alice", "Alertmanager", png(8, 8))
        val picture = 0xFF000000.toInt()
        val colors =
            listOf(
                    """{"summary": "Synced", "urgency": "high"}""" to picture,
                    """{"summary": "Down", "disposition": "problem"}""" to FIRE,
                    """{"summary": "Up", "disposition": "resolved"}""" to GOOD,
                    """{"summary": "Restart?", "choices": ["Yes"]}""" to ACCENT,
                )
                .map { (message, color) ->
                    awaitPosted(send(message)).senderIcon().getPixel(BADGE_X, BADGE_Y) to color
                }
        assertEquals(colors.map { it.second }, colors.map { it.first })
    }

    @Test
    fun anUrgentMessageSaysHowManyTimesItHasRung() {
        val posted = awaitPosted(send("""{"summary": "UPS on battery", "urgency": "urgent"}"""))
        assertEquals(listOf("@alice: UPS on battery\nRang 1 of 4 times"), posted.messages)
    }

    @Test
    fun aReplacementTakesOverTheNotificationQuietlyWhenLow() {
        val firing = send("""{"summary": "Disk full", "key": "disk"}""")
        awaitPosted(firing)
        val resolved = send("""{"summary": "Disk fine", "key": "disk", "urgency": "low"}""")
        val shown = awaitPosted(firing) { it.messages == listOf("@alice: Disk fine") }
        assertEquals(listOf(true, null), listOf(shown.silent, posted(resolved)))
    }

    @Test
    fun aReplacementOfOneSwipedAwayIsANotificationOfItsOwn() {
        val firing = send("""{"summary": "Disk full", "key": "disk"}""")
        awaitPosted(firing).swipe()
        awaitCleared(firing)
        val resolved = send("""{"summary": "Disk fine", "key": "disk"}""")
        assertEquals(false, awaitPosted(resolved).silent)
    }

    @Test
    fun theFirstImageComesBeforeTheText() {
        val id =
            send(
                message(
                    "Severance S02E04 is ready",
                    attachments = listOf("poster.png" to png(20, 30)),
                )
            )
        val image = awaitPosted(id).style().messages.first()
        val bytes = app.contentResolver.openInputStream(image.dataUri!!)!!.use { it.readBytes() }
        assertEquals(listOf("image/png", png(20, 30).size), listOf(image.dataMimeType, bytes.size))
    }

    @Test
    fun readingManyAtOnceClearsEachNotification() {
        val ids = listOf("Backup done", "Disk fine").map { send("""{"summary": "$it"}""") }
        ids.forEach(::awaitPosted)
        val body = ids.joinToString(", ", """{"exchanges": [""", "]}") { "\"$it\"" }
        server.call("POST", "/@bob/read", bob, body)
        ids.forEach(::awaitCleared)
    }
}

private fun Notification.style() =
    NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(this)!!

/** The icon of the sender of the notification's first message, as Android draws it. */
private fun Notification.senderIcon(): Bitmap {
    val icon = checkNotNull(style().messages.first().person?.icon) { "the sender has no icon" }
    return (icon.loadDrawable(app) as BitmapDrawable).bitmap
}

/**
 * A pixel of a sender's icon of 192 pixels a side that is inside the badge on its upper right and
 * clear of every badge's glyph.
 */
private const val BADGE_X = 160
private const val BADGE_Y = 52

/** The light palette's colors of a problem's, a resolution's, and a question's badge. */
private const val FIRE = 0xFFC2412D.toInt()
private const val GOOD = 0xFF2F7D4F.toInt()
private const val ACCENT = 0xFF3D5A73.toInt()
