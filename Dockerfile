FROM rust:1.81.0-alpine3.20 AS builder
WORKDIR /usr/src/app
COPY . .
RUN cargo install --path .

FROM alpine:3.20
COPY --from=builder /usr/local/cargo/bin/app /usr/local/bin/app
ENTRYPOINT [ "app" ]
