package sh.transom.signin

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import java.net.Inet6Address
import java.net.InetAddress
import java.net.UnknownHostException

/**
 * Whether [host] resolves to an address that is not public. Android 17 and later refuse the app's
 * connections to such an address until the person grants the local network permission, and a server
 * at home usually has one. Loopback counts as not public too: Android exempts it, so asking there
 * is needless but harmless. A name that does not resolve is not on the local network, since the
 * permission would not help the app reach it.
 */
internal fun onLocalNetwork(host: String): Boolean =
    try {
        InetAddress.getAllByName(host).any { !it.isPublic() }
    } catch (_: UnknownHostException) {
        false
    }

/**
 * Whether the internet routes to this address. Java's [InetAddress.isSiteLocalAddress] covers the
 * private IPv4 ranges but, for IPv6, only the retired `fec0::/10`, so unique local addresses,
 * `fc00::/7`, are checked by their first byte.
 */
internal fun InetAddress.isPublic(): Boolean =
    !isLoopbackAddress &&
        !isAnyLocalAddress &&
        !isLinkLocalAddress &&
        !isSiteLocalAddress &&
        !isMulticastAddress &&
        !(this is Inet6Address && address[0].toInt() and 0xfe == 0xfc)

/**
 * Whether the app should ask for the local network permission before it tries [problem]'s server
 * again: the server is on the local network, the phone's [sdk] is Android 17 or later, which keeps
 * the app from reaching that network without the permission, and the person has not granted it.
 */
internal fun Context.asksForLocalNetwork(
    problem: Problem?,
    sdk: Int = Build.VERSION.SDK_INT,
): Boolean =
    problem is Problem.Unreachable &&
        problem.local &&
        sdk >= Build.VERSION_CODES.CINNAMON_BUN &&
        checkSelfPermission(LOCAL_NETWORK) != PackageManager.PERMISSION_GRANTED

/**
 * The permission Android 17 and later require before the app connects to the local network. The
 * compiler copies the string into the app, and [asksForLocalNetwork] asks for it only on Android 17
 * or later, so an older Android never sees it.
 */
@SuppressLint("InlinedApi")
internal const val LOCAL_NETWORK = Manifest.permission.ACCESS_LOCAL_NETWORK
