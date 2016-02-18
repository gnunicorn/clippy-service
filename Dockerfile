FROM debian:jessie
MAINTAINER Benjamin Kampmann (http://github.com/ligthyear)

RUN apt-get install -y build-essential g++ pgp python perl make curl git libssl-dev

RUN curl -sO https://static.rust-lang.org/rustup.sh
RUN bash rustup.sh --yes --channel=nightly
ENV LD_LIBRARY_PATH $LD_LIBRARY_PATH:/usr/local/lib

# Setup app
COPY app ~/app

WORKDIR ~/app
RUN cargo build

EXPOSE 8080

CMD cd ~/app && cargo run
