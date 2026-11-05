package sh.transom.signin

import com.sun.net.httpserver.HttpServer
import java.net.InetSocketAddress
import java.net.ServerSocket
import org.junit.rules.ExternalResource

/**
 * A web server that is not transom, answering every request with [status] and [body], for the
 * sign-in to meet at an address that was mistyped or misremembered.
 */
class OtherServer(private val status: Int = 200, private val body: String = "<h1>Welcome</h1>") :
    ExternalResource() {
    lateinit var url: String
        private set

    private lateinit var server: HttpServer

    override fun before() {
        server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/") { exchange ->
            val bytes = body.toByteArray()
            exchange.sendResponseHeaders(status, bytes.size.toLong())
            exchange.responseBody.use { it.write(bytes) }
        }
        server.start()
        url = "http://127.0.0.1:${server.address.port}"
    }

    override fun after() = server.stop(0)
}

/** An address where nothing listens: a port that was free a moment ago. */
fun nowhere(): String = "http://127.0.0.1:${ServerSocket(0).use { it.localPort }}"
