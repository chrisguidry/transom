# The OCI keys of every image this repository publishes. CI fills the
# variables from the commit it builds, and `docker buildx bake --push`
# writes each key as a config label and as an annotation on the image
# index and each image manifest. The keys live here and not in the
# Dockerfile, so a change to a key invalidates no layer cache, and a
# fork that builds the Dockerfile on its own does not claim to be this
# repository's image.

# SOURCE is the repository's URL, REVISION the commit, VERSION the
# release or development version, and CREATED the commit's committer
# time in RFC 3339. A local build leaves them empty, and an empty key
# is left out.
variable "SOURCE" { default = "" }
variable "REVISION" { default = "" }
variable "VERSION" { default = "" }
variable "CREATED" { default = "" }

# The keys that are the same for every image of the repository.
variable "URL" { default = "https://transom.sh" }
variable "LICENSES" { default = "MIT" }
variable "VENDOR" { default = "Chris Guidry" }

# oci returns the keys of one image: the commit's keys, the
# repository's keys, and the image's own keys, with the image's keys
# last so that they win. A commit key that is empty is left out, so a
# local build writes no source, revision, version, or created. Any
# other key that is "" is written as "", which replaces a label of the
# same key that the base image set.
function "oci" {
  params = [image]
  result = { for key, value in merge(
    { for key, value in {
      source   = SOURCE
      revision = REVISION
      version  = VERSION
      created  = CREATED
    } : key => value if value != "" },
    {
      url      = URL
      licenses = LICENSES
      vendor   = VENDOR
    },
    image,
  ) : "org.opencontainers.image.${key}" => value }
}

# oci_labels and oci_annotations write the keys of oci at the two
# places that readers look: the config, which docker inspect, syft,
# and Trivy read, and the index and manifests, which ghcr.io reads for
# a multi-platform image. A label passes into every image built FROM
# this one and an annotation does not, so a downstream image keeps
# none of these annotations.
function "oci_labels" {
  params = [image]
  result = oci(image)
}

# A local build with --load exports no image index, and BuildKit refuses
# an index annotation there, so only a CI build, which sets REVISION,
# writes annotations.
function "oci_annotations" {
  params = [image]
  result = REVISION == "" ? [] : [for key, value in oci(image) : "index,manifest:${key}=${value}"]
}

# CACHE_FROM and CACHE_TO turn on the GitHub Actions layer cache, which
# a build outside Actions cannot reach. The build that checks a change
# writes the layers, and the build that publishes reads them and
# rebuilds nothing.
variable "CACHE_FROM" { default = false }
variable "CACHE_TO" { default = false }

# A published image carries BuildKit's provenance and an SBOM. A local
# build with --load cannot hold attestations, so only a build that sets
# REVISION attaches them.
function "attestations" {
  params = []
  result = REVISION == "" ? [] : ["type=provenance,mode=max", "type=sbom"]
}

variable "transom" {
  default = {
    title         = "transom"
    description   = "The transom server, which lets a program ask a person a question and get the answer back"
    documentation = "https://transom.sh"
  }
}

variable "transom_gateway" {
  default = {
    title         = "transom-gateway"
    description   = "The transom wake-up gateway, which sends an empty FCM message to a phone so the transom app syncs from its server"
    documentation = "https://github.com/chrisguidry/transom/blob/main/gateway/README.md"
  }
}

group "default" {
  targets = ["transom", "transom-gateway"]
}

target "transom" {
  context     = "."
  dockerfile  = "server/Dockerfile"
  tags        = ["ghcr.io/chrisguidry/transom:${VERSION == "" ? "dev" : VERSION}"]
  platforms   = ["linux/amd64"]
  labels      = oci_labels(transom)
  annotations = oci_annotations(transom)
  attest      = attestations()
  cache-from  = CACHE_FROM ? ["type=gha,scope=image"] : []
  cache-to    = CACHE_TO ? ["type=gha,scope=image,mode=max"] : []
}

target "transom-gateway" {
  context     = "."
  dockerfile  = "gateway/Dockerfile"
  tags        = ["ghcr.io/chrisguidry/transom-gateway:${VERSION == "" ? "dev" : VERSION}"]
  platforms   = ["linux/amd64"]
  labels      = oci_labels(transom_gateway)
  annotations = oci_annotations(transom_gateway)
  attest      = attestations()
  cache-from  = CACHE_FROM ? ["type=gha,scope=gateway"] : []
  cache-to    = CACHE_TO ? ["type=gha,scope=gateway,mode=max"] : []
}
