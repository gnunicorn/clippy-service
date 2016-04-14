FROM debian:jessie
MAINTAINER Benjamin Kampmann (http://github.com/ligthyear)

RUN apt-get update -y && apt-get upgrade -y
RUN apt-get install -y build-essential sudo g++ pgp python perl make curl git libssl-dev cpulimit

RUN curl -sO https://static.rust-lang.org/rustup.sh
RUN bash rustup.sh --yes --channel=nightly
ENV LD_LIBRARY_PATH $LD_LIBRARY_PATH:/usr/local/lib

# Setup app
COPY . ~/app

WORKDIR ~/app

# install firejail
RUN etc/install_firejail.sh

# build service
RUN cargo build --verbose

EXPOSE 5000

CMD cargo run
