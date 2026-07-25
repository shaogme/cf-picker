# 第一阶段：编译构建环境
FROM rust:1.97-alpine AS builder

WORKDIR /usr/src/app

# 安装编译所需的系统依赖项
RUN apk add --no-cache \
    musl-dev \
    pkgconfig \
    openssl-dev \
    openssl-libs-static \
    make \
    gcc \
    g++ \
    bash

# 复制源码
COPY . .

# 编译 release 产物
RUN cargo build --release -p cf-picker-cli

# 第二阶段：运行环境
FROM alpine:latest AS runner

WORKDIR /app

# 安装运行时所需的基础证书及 OpenSSL 依赖
RUN apk add --no-cache \
    ca-certificates \
    openssl

# 从构建阶段复制二进制文件及相关配置、IP 列表文件
COPY --from=builder /usr/src/app/target/release/cf-picker-cli /app/cf-picker
COPY --from=builder /usr/src/app/config.toml /app/config.toml
COPY --from=builder /usr/src/app/ip.txt /app/ip.txt
COPY --from=builder /usr/src/app/ipv6.txt /app/ipv6.txt

# 设置默认入口点
ENTRYPOINT ["/app/cf-picker"]

