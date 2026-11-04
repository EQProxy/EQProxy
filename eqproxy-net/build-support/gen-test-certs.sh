#!/bin/sh
# Ephemeral test certificates. They live in the build's OUT_DIR (never in the
# repository) and are regenerated whenever a test binary is compiled. The
# private keys are throwaway RSA keys for loopback tests, not credentials.
set -eu

out=${1:?usage: gen-test-certs.sh <output-dir>}
mkdir -p "$out"
cd "$out"

# name, CN, subjectAltName
self_signed() {
    openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 3650 \
        -keyout "$1.key" -out "$1.pem" -subj "/CN=$2" \
        -addext "subjectAltName=$3" >/dev/null 2>&1
}

self_signed cert localhost "DNS:localhost,IP:127.0.0.1"
mv cert.key key.pem
self_signed alpha alpha.example.test "DNS:alpha.example.test"
self_signed beta beta.example.test "DNS:beta.example.test"

openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 3650 \
    -keyout mtls-ca.key -out mtls-ca.pem -subj "/CN=EQProxy Test CA" \
    -addext "basicConstraints=critical,CA:TRUE" >/dev/null 2>&1

openssl req -new -newkey rsa:2048 -nodes -sha256 \
    -keyout mtls-server.key -out server.csr -subj "/CN=localhost" >/dev/null 2>&1
cat > server.ext <<'EOF'
subjectAltName=DNS:localhost,IP:127.0.0.1
basicConstraints=CA:FALSE
keyUsage=digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
EOF
openssl x509 -req -sha256 -in server.csr -CA mtls-ca.pem -CAkey mtls-ca.key \
    -CAcreateserial -out mtls-server.pem -days 3650 -extfile server.ext >/dev/null 2>&1

openssl req -new -newkey rsa:2048 -nodes -sha256 \
    -keyout mtls-client.key -out client.csr -subj "/CN=eqproxy-test-client" >/dev/null 2>&1
cat > client.ext <<'EOF'
basicConstraints=CA:FALSE
keyUsage=digitalSignature
extendedKeyUsage=clientAuth
EOF
openssl x509 -req -sha256 -in client.csr -CA mtls-ca.pem -CAkey mtls-ca.key \
    -CAcreateserial -out mtls-client.pem -days 3650 -extfile client.ext >/dev/null 2>&1

rm -f server.csr server.ext client.csr client.ext mtls-ca.srl
chmod 600 ./*.key
