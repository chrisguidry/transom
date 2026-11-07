variable "project_id" {
  description = "The Google Cloud project id, unique across all of Google Cloud"
  type        = string
  default     = "transom-wake"
}

# Every key that signs a build of sh.transom: the release key that signs
# the APKs on GitHub releases. These match the fingerprints in
# site/public/.well-known/assetlinks.json.
variable "android_signing_sha256" {
  description = "The SHA-256 fingerprints of the keys that sign sh.transom, as keytool prints them"
  type        = list(string)
  default = [
    "43:9B:0D:00:20:3D:3C:E2:04:F3:A0:11:D0:52:52:9A:F0:ED:B2:7E:6B:5A:38:F4:8A:2C:50:4F:50:41:DB:37",
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

variable "billing_account" {
  description = "The billing account that pays for the project"
  type        = string
  default     = "019067-3B1506-31E7D1"
}

variable "region" {
  description = "Where the gateway runs, and the registry it pulls its image through"
  type        = string
  default     = "us-central1"
}

# The gateway's image, which image.auto.tfvars sets: CI writes each new
# build's tag and digest there and applies, and so does a person rolling
# back. The digest pins the image, and the tag says which build it is.
variable "gateway_image_tag" {
  description = "The tag of ghcr.io/chrisguidry/transom-gateway that the service runs"
  type        = string
}

variable "gateway_image_digest" {
  description = "The digest of that tag's image, as sha256:<hex>"
  type        = string

  validation {
    condition     = can(regex("^sha256:[0-9a-f]{64}$", var.gateway_image_digest))
    error_message = "The digest is sha256: and 64 hex digits."
  }
}

# The repository whose workflows may act in the project. GitHub
# reuses a name after a repository is deleted, and never an id.
variable "github_repository" {
  description = "The GitHub repository whose workflows plan and apply this directory, as owner/name"
  type        = string
  default     = "chrisguidry/transom"
}

variable "github_repository_id" {
  description = "The numeric id of github_repository"
  type        = string
  default     = "1444620929"
}
