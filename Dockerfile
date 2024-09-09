FROM rust:1.81.0-alpine3.20 AS builder
RUN apk add --no-cache musl-dev libressl-dev
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release

FROM alpine:3.20
COPY --from=builder /usr/src/app/target/release/htmx-ssh-games /usr/local/bin/htmx-ssh-games
ENTRYPOINT [ "app" ]
