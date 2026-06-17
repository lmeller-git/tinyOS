FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    qemu-system-x86 \
    xorriso \

    novnc \
    websockify \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN rustup install nightly
RUN rustup default nightly
RUN rustup component add rust-src llvm-tools-preview
RUN rustup target add x86_64-unknown-none

WORKDIR /os
COPY . .

ENV RUST_PROFILE=release
ENV LIBGCC_PATH=/usr/lib/gcc/x86_64-linux-gnu/13/libgcc.a

RUN make all

EXPOSE 8080

ENTRYPOINT ["sh", "-c", "websockify --web /usr/share/novnc 8080 localhost:5900 & make run RUST_PROFILE=release QEMUFLAGS='-display vnc=:0 -vga std'"]
