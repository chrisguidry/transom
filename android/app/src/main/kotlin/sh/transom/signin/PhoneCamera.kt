package sh.transom.signin

import androidx.camera.compose.CameraXViewfinder
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.core.SurfaceRequest
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.lifecycle.awaitInstance
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LocalLifecycleOwner
import java.util.concurrent.Executors
import kotlinx.coroutines.awaitCancellation

/**
 * The phone's camera, through CameraX, the back one when it has one: its preview, and each frame's
 * luminance read for a QR code by [qrText]. Robolectric has no camera, so the build file excludes
 * this object from Kover's coverage, and the emulator runs it. Everything that can run without a
 * camera is outside it.
 */
internal object PhoneCamera : Camera {
    @Composable
    override fun Viewfinder(onText: (String) -> Unit, modifier: Modifier) {
        val context = LocalContext.current
        val owner = LocalLifecycleOwner.current
        val latest by rememberUpdatedState(onText)
        var request by remember { mutableStateOf<SurfaceRequest?>(null) }
        LaunchedEffect(owner) {
            val provider = ProcessCameraProvider.awaitInstance(context)
            val preview = Preview.Builder().build().apply { setSurfaceProvider { request = it } }
            // Frames that arrive while one is read are dropped, so reading never falls behind.
            val analysis =
                ImageAnalysis.Builder()
                    .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                    .build()
            val reader = Executors.newSingleThreadExecutor()
            analysis.setAnalyzer(reader) { image ->
                val plane = image.planes[0]
                val text = image.use {
                    qrText(Frame(plane.buffer, plane.rowStride, it.width, it.height))
                }
                text?.let { context.mainExecutor.execute { latest(it) } }
            }
            // A tablet may have only a camera on its screen's side, which can scan a code too.
            val camera =
                if (provider.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA)) {
                    CameraSelector.DEFAULT_BACK_CAMERA
                } else {
                    CameraSelector.DEFAULT_FRONT_CAMERA
                }
            provider.bindToLifecycle(owner, camera, preview, analysis)
            try {
                awaitCancellation()
            } finally {
                provider.unbind(preview, analysis)
                reader.shutdown()
            }
        }
        request?.let { CameraXViewfinder(it, modifier) }
    }
}
