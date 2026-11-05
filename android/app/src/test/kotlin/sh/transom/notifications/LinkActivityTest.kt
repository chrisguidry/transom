package sh.transom.notifications

import android.content.Intent
import android.net.Uri
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import sh.transom.signin.app

/** A notification's Open link reaches the browser through transom's own activity. */
@RunWith(RobolectricTestRunner::class)
class LinkActivityTest {
    private fun launched(link: Uri?) =
        Robolectric.buildActivity(
                LinkActivity::class.java,
                Intent(app, LinkActivity::class.java).setData(link),
            )
            .setup()
            .get()

    @Test
    fun itOpensTheLinkAndFinishes() {
        val activity = launched(Uri.parse("https://ci.example.com/run/7"))
        val opened = shadowOf(activity).nextStartedActivity
        assertEquals(Intent.ACTION_VIEW, opened.action)
        assertEquals("https://ci.example.com/run/7", opened.dataString)
        assertTrue(activity.isFinishing)
    }

    @Test
    fun withoutALinkItOnlyFinishes() {
        val activity = launched(null)
        assertNull(shadowOf(activity).nextStartedActivity)
        assertTrue(activity.isFinishing)
    }
}
