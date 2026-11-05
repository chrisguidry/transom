package sh.transom

import android.Manifest
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.os.ParcelFileDescriptor
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.BySelector
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.UiObject2
import androidx.test.uiautomator.Until
import java.io.ByteArrayOutputStream
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.After
import org.junit.Test
import sh.transom.delivery.Delivery
import sh.transom.settings.Preferences

/**
 * The whole path a person takes, on a phone: they sign in through the Custom Tab, leave the app,
 * and answer a question from its notification, and the asker sees their choice on the server.
 *
 * It runs on an emulator against the servers `end-to-end.sh` starts, never under `./gradlew check`.
 */
class EndToEndTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val device = UiDevice.getInstance(instrumentation)
    private val context: Context = instrumentation.targetContext
    private val host = HostServer()

    @Test
    fun aQuestionIsAnsweredFromItsNotification() {
        val asker = host.tokenFor("asker")
        host.accept(PERSON, host.tokenFor(PERSON), asker)
        prepare()

        signIn()
        leave()

        val id =
            host.send(
                asker,
                PERSON,
                """{"summary": "Deploy?", "choices": [
                    {"label": "Deploy now", "id": "deploy"}, {"label": "Hold", "id": "hold"}]}""",
            )
        await("the question's notification") { notifications().any { it.tag == id } }
        inTheShade(By.text("Deploy now")).click()

        await("the server to have the reply") {
            val exchange = host.exchange(id, asker)
            val reply = exchange["reply"] as? JsonObject
            exchange.getValue("state").jsonPrimitive.content == "replied" &&
                reply?.getValue("choice")?.jsonPrimitive?.content == "deploy"
        }
        await("the notification to clear") { notifications().none { it.tag == id } }
    }

    /** Closes the shade, which would otherwise cover the app in the next run on this emulator. */
    @After
    fun closeTheShade() {
        shell("cmd statusbar collapse")
    }

    /**
     * Approves the app's App Link, which Android cannot verify for a build signed with a key that
     * transom.sh does not name, and grants what the app would ask for, so no dialog stands between
     * the steps. A shade left open by anything before would cover the app, so it closes first.
     *
     * The app delivers through the direct connection, which CI's build always does, since it has no
     * Firebase project. A build with one would otherwise choose Google Play services, and the test
     * would then wait on FCM and transom.sh's wake-up gateway rather than on the servers
     * `end-to-end.sh` runs.
     */
    private fun prepare() {
        shell("cmd statusbar collapse")
        Preferences(context).delivery = Delivery.Connection
        shell(
            "pm set-app-links-user-selection --user 0 --package ${context.packageName} true $HOST"
        )
        instrumentation.uiAutomation.grantRuntimePermission(
            context.packageName,
            Manifest.permission.POST_NOTIFICATIONS,
        )
    }

    /**
     * Signs in as the person would: the server's address and Continue, then the issuer's link on
     * transom's page in the Custom Tab. The test issuer signs its default person in at once, and
     * transom sends the Custom Tab to the App Link, which brings the app back to its inbox.
     */
    private fun signIn() {
        val launch =
            checkNotNull(context.packageManager.getLaunchIntentForPackage(context.packageName))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK)
        context.startActivity(launch)
        awaitObject(By.clazz("android.widget.EditText")).text = host.url
        awaitObject(By.text("Continue")).click()
        awaitObject(By.text("Continue with Test issuer")).click()
        awaitObject(By.pkg(context.packageName).text("@$PERSON"))
    }

    /**
     * Leaves the app for the launcher, as a person would before the question arrives, and goes on
     * once the app is off the screen. The home key also closes the shade, which [inTheShade] opens
     * again if the close arrives late.
     */
    private fun leave() {
        device.pressHome()
        check(device.wait(Until.gone(By.pkg(context.packageName)), WAIT_MS) == true) {
            "the app was still on screen after ${WAIT_MS / 1000} seconds. The screen:\n${screen()}"
        }
    }

    /**
     * What in the notification shade matches [selector], once the shade is open and has stopped
     * moving, so a tap lands where the object is drawn. The shade opens again each time it shows no
     * match within [SHADE_MS], since something may close it as it opens, such as the home key when
     * its close of the system's dialogs reaches the shade late.
     */
    private fun inTheShade(selector: BySelector): UiObject2 {
        val deadline = System.nanoTime() + WAIT_MS * 1_000_000
        while (System.nanoTime() < deadline) {
            device.openNotification()
            if (device.wait(Until.hasObject(selector), SHADE_MS) == true) {
                device.waitForIdle()
                device.findObject(selector)?.let {
                    return it
                }
            }
        }
        error(
            "the shade showed no $selector in ${WAIT_MS / 1000} seconds. The screen:\n${screen()}"
        )
    }

    private fun notifications() =
        context.getSystemService(NotificationManager::class.java).activeNotifications.toList()

    /**
     * What on screen matches [selector], once something does. A failure carries the screen's
     * hierarchy, since CI's emulator is gone by the time anyone reads it.
     */
    private fun awaitObject(selector: BySelector): UiObject2 =
        checkNotNull(device.wait(Until.findObject(selector), WAIT_MS)) {
            "nothing on screen matched $selector in ${WAIT_MS / 1000} seconds. The screen:\n${screen()}"
        }

    /** The screen's window hierarchy, for a failure's message. */
    private fun screen(): String =
        ByteArrayOutputStream().also { device.dumpWindowHierarchy(it) }.toString()

    private fun shell(command: String): String =
        instrumentation.uiAutomation.executeShellCommand(command).use {
            ParcelFileDescriptor.AutoCloseInputStream(it).bufferedReader().readText()
        }

    private fun await(what: String, condition: () -> Boolean) {
        val deadline = System.nanoTime() + WAIT_MS * 1_000_000
        while (!condition()) {
            check(System.nanoTime() < deadline) { "waited ${WAIT_MS / 1000} seconds for $what" }
            Thread.sleep(200)
        }
    }

    private companion object {
        /** The person the test issuer signs in when the authorization request names no one. */
        const val PERSON = "tester"

        /** The host of the app's App Link, which transom sends the sign-in's code to. */
        const val HOST = "transom.sh"

        const val WAIT_MS = 30_000L

        /** How long the shade has to show what the test looks for before it opens again. */
        const val SHADE_MS = 5_000L
    }
}
