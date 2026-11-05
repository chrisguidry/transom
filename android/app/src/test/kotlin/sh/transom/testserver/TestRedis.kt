package sh.transom.testserver

/**
 * The Redis that every [TestServer] in this test JVM keeps its keys in, each server under a prefix
 * of its own.
 *
 * `test-redis` starts it in a Docker container on a free port, or names the Redis at
 * `TRANSOM_TEST_REDIS_URL` when that is set, and keeps the container until its standard input
 * closes. This JVM holds that input open for as long as it runs, and the kernel closes it when the
 * JVM exits, however it exits, so no run leaves a container behind and parallel runs never share
 * one. Gradle names the binary in the system property `test-redis.binary`.
 */
internal object TestRedis {
    private const val VARIABLE = "TRANSOM_TEST_REDIS_URL"

    /** Long enough to pull the image on a machine that has never run it. */
    private const val STARTUP_SECONDS = 150L

    // The holder is kept for the JVM's life, and with it the pipe to its standard input.
    private val holder: Spawned by lazy {
        spawn(
            listOf(TestServer.property("test-redis.binary")),
            environment = System.getenv(VARIABLE)?.let { mapOf(VARIABLE to it) } ?: emptyMap(),
            Watched.STDOUT,
        ) {
            // test-redis prints the URL once Redis answers, and nothing before it.
            it.trim()
        }
    }

    val url: String by lazy { holder.awaitReady(STARTUP_SECONDS) }
}
