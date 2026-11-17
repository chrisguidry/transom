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

# Throwaway change for a CI test.
