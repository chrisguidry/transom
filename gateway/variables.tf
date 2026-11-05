variable "project_id" {
  description = "The Google Cloud project id, unique across all of Google Cloud"
  type        = string
  default     = "transom-wake"
}

# Every key that signs a build of sh.transom: the release key that signs
# the APKs on GitHub releases, and the debug key of the machine the app
# is developed on. These match the fingerprints in
# site/public/.well-known/assetlinks.json.
variable "android_signing_sha256" {
  description = "The SHA-256 fingerprints of the keys that sign sh.transom, as keytool prints them"
  type        = list(string)
  default = [
    "43:9B:0D:00:20:3D:3C:E2:04:F3:A0:11:D0:52:52:9A:F0:ED:B2:7E:6B:5A:38:F4:8A:2C:50:4F:50:41:DB:37",
    "EC:06:E8:E9:39:0E:D0:0D:A3:7E:93:C0:B8:17:8E:61:F4:6A:63:C4:02:E1:E2:91:B0:8B:94:52:53:21:30:D7",
  ]

  validation {
    condition = alltrue([
      for fingerprint in var.android_signing_sha256 :
      can(regex("^[0-9A-Fa-f]{64}$", replace(fingerprint, ":", "")))
    ])
    error_message = "Each fingerprint is 32 bytes of hex, with or without colons."
  }
}

variable "kubeconfig_path" {
  description = "The kubeconfig that reaches the cluster that runs the gateway"
  type        = string
  default     = "~/.kube/config"
}

variable "kube_context" {
  description = "The kubeconfig context of the cluster that runs the gateway"
  type        = string
  default     = "public-sites"
}
