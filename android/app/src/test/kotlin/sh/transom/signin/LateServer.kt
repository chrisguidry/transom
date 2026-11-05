package sh.transom.signin

import com.sun.net.httpserver.HttpServer
import java.net.InetSocketAddress
import org.junit.rules.ExternalResource

/**
 * A server at [url] that answers only once [start] is called, as a server on the local network does
 * once Android lets the app reach it. It answers each request with the request's query, which
 * carries the sign-in's PKCE challenge as transom's sign-in page does, so the app takes it for
 * transom.
 */
class LateServer : ExternalResource() {
    val url = nowhere()

    private val server = HttpServer.create()

    fun start() {
        server.bind(InetSocketAddress("127.0.0.1", url.substringAfterLast(':').toInt()), 0)
        server.createContext("/") { exchange ->
            val bytes = exchange.requestURI.rawQuery.orEmpty().toByteArray()
            exchange.sendResponseHeaders(200, bytes.size.toLong())
            exchange.responseBody.use { it.write(bytes) }
        }
        server.start()
    }

    override fun after() = server.stop(0)
}
