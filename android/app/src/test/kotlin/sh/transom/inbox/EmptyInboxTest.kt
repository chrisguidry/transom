package sh.transom.inbox

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import sh.transom.LaunchedApp
import sh.transom.signin.Session
import sh.transom.signin.awaitText
import sh.transom.testserver.NOWHERE
import sh.transom.theme.DarkPalette
import sh.transom.theme.LightPalette

/** The inbox of a signed-in app, which has nothing in it until the app syncs. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class EmptyInboxTest {
    @get:Rule val compose = createEmptyComposeRule()
    @get:Rule val launched = LaunchedApp()

    @Before
    fun signIn() {
        launched.signedIn(Session(NOWHERE, "transom_token", "alice"))
    }

    @Test
    fun showsTheWordmark() {
        compose.onNodeWithText("transom").assertIsDisplayed()
    }

    @Test
    fun headsThePageWithTheName() {
        compose.onNode(hasText("@alice") and isHeading()).assertIsDisplayed()
    }

    @Test
    fun saysThereIsNothingHereYet() {
        compose.awaitText("Nothing here yet.", substring = true)
        compose
            .onNodeWithText("Nothing here yet. Messages sent to @alice show up here.")
            .assertIsDisplayed()
    }

    @Test
    @Config(qualifiers = "w700dp-h1000dp")
    fun aTabletsBarNamesWhoIsSignedInBesideTheHeading() {
        compose.onAllNodesWithText("@alice").assertCountEquals(2)
    }

    @Test
    fun paintsLightPaper() {
        assertEquals(LightPalette.paper, cornerColor())
    }

    @Test
    @Config(qualifiers = "night")
    fun paintsDarkPaperWhenTheSystemIsDark() {
        assertEquals(DarkPalette.paper, cornerColor())
    }

    /** The color of the screen's bottom-right corner, where nothing but the page is drawn. */
    private fun cornerColor(): Color {
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        return pixels[pixels.width - 1, pixels.height - 1]
    }
}
