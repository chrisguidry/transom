package sh.transom.testserver

import java.io.InputStream
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException

/**
 * A child process, and the first line of its output that [ready] accepts.
 *
 * A thread reads [output] to its end, so the process never blocks on a full pipe, and keeps every
 * line for the message of a failure.
 */
internal class Spawned(
    private val name: String,
    private val process: Process,
    output: InputStream,
    ready: (String) -> String?,
) {
    private val lines = mutableListOf<String>()
    private val readiness = CompletableFuture<String>()

    init {
        Thread(
                {
                    output.bufferedReader().useLines { all ->
                        all.forEach { line ->
                            synchronized(lines) { lines.add(line) }
                            ready(line)?.let { readiness.complete(it) }
                        }
                    }
                    readiness.completeExceptionally(
                        IllegalStateException("$name exited before it was ready:\n${log()}")
                    )
                },
                "$name output",
            )
            .apply { isDaemon = true }
            .start()
    }

    /** What [ready] made of the line that showed the process ready, waiting [seconds] for it. */
    fun awaitReady(seconds: Long): String =
        try {
            readiness.get(seconds, TimeUnit.SECONDS)
        } catch (_: TimeoutException) {
            throw IllegalStateException("$name was not ready after $seconds seconds:\n${log()}")
        } catch (failure: ExecutionException) {
            throw failure.cause!!
        }

    fun log(): String = synchronized(lines) { lines.joinToString("\n") }

    /** Kills the process with SIGKILL, so it stops at once, without finishing what it was doing. */
    fun kill() {
        process.destroyForcibly().waitFor()
    }

    /** Asks the process to stop with SIGTERM, and kills it if it has not stopped in 5 seconds. */
    fun stop() {
        process.destroy()
        if (!process.waitFor(5, TimeUnit.SECONDS)) {
            process.destroyForcibly().waitFor()
        }
    }
}

/** The output stream of a child process that shows when it is ready. */
internal enum class Watched {
    STDOUT,
    STDERR,
}

/**
 * Starts [command] with [environment] and none of the test runner's own `TRANSOM_` variables, so a
 * developer's settings cannot reach the test server. The stream it does not watch goes to the test
 * JVM's own, so a crash written there still shows in Gradle's output.
 */
internal fun spawn(
    command: List<String>,
    environment: Map<String, String>,
    watched: Watched,
    ready: (String) -> String?,
): Spawned {
    val builder = ProcessBuilder(command)
    builder.environment().keys.removeAll { it.startsWith("TRANSOM_") }
    builder.environment().putAll(environment)
    when (watched) {
        Watched.STDOUT -> builder.redirectError(ProcessBuilder.Redirect.INHERIT)
        Watched.STDERR -> builder.redirectOutput(ProcessBuilder.Redirect.INHERIT)
    }
    val process = builder.start()
    val output =
        when (watched) {
            Watched.STDOUT -> process.inputStream
            Watched.STDERR -> process.errorStream
        }
    return Spawned(command.first().substringAfterLast('/'), process, output, ready)
}
