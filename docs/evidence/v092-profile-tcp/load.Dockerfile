FROM alpine:3.20
COPY tcp_load /tcp_load
COPY probe /probe
ENTRYPOINT ["/tcp_load"]
