FROM rust:latest AS build
WORKDIR /usr/src/imscale-service
COPY Cargo.toml Cargo.lock .
COPY src src
RUN cargo install --path .

FROM debian
COPY --from=build /usr/local/cargo/bin/imscale-service /imscale-service
COPY public /public
WORKDIR /
CMD ["/imscale-service"]
