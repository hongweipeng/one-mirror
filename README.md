# one-mirror
公共镜像代理

## 项目介绍

**one-mirror** 是一个用 Rust 编写的轻量级公共镜像反向代理服务，一个程序搞定所有镜像加速。

基于 [axum](https://github.com/tokio-rs/axum) + [hyper](https://github.com/hyperium/hyper) + [tokio](https://github.com/tokio-rs/tokio) 构建，编译为单一二进制文件，资源占用极小，甚至可以在小内存的 LXC 容器中运行。

### 特性

- **单一二进制** — 编译产物仅一个可执行文件，无运行时依赖，部署极简
- **低资源占用** — Rust 原生性能，内存占用极低，适合资源受限环境
- **全场景覆盖** — 一站式代理 Linux 发行版、编程语言包管理器、容器镜像等常见上游源
- **并发控制** — 内置信号量限流，防止上游请求过载
- **自动重定向** — 透明跟随上游 30x 重定向，客户端无感知
- **Docker 镜像代理** — 自动处理 Docker Registry 认证流程，重写 `WWW-Authenticate` 头
- **CentOS 历史版本修复** — 自动将已归档的 CentOS 5/6 版本请求路由至 `archive.kernel.org`

### 运行参数

| 参数 | 默认值     | 说明 |
|------|---------|------|
| `--server-host` | `[::]`  | 监听地址 |
| `--server-port` | `13400` | 监听端口 |
| `--max-concurrency` | `1000`  | 上游代理请求最大并发数 |

# 起步
假设域名使用 `mirrors.xxx.com`

# debian
debian12
```
cp -a /etc/apt/sources.list.d/debian.sources /etc/apt/sources.list.d/debian.sources.bak
sed -i 's@deb.debian.org@mirrors.xxx.com@g' /etc/apt/sources.list.d/debian.sources
sed -i "s@security.debian.org@mirrors.xxx.com@g" /etc/apt/sources.list.d/debian.sources
```

debian11 及其之前
```
cp -a /etc/apt/sources.list /etc/apt/sources.list.bak
sed -i "s@deb.debian.org@mirrors.xxx.com@g" /etc/apt/sources.list
sed -i "s@security.debian.org@mirrors.xxx.com@g" /etc/apt/sources.list
```


# ubuntu
```
cp /etc/apt/sources.list /etc/apt/sources.list.bak \
    && sed -i "s@archive.ubuntu.com@mirrors.xxx.com@g" /etc/apt/sources.list \
    && sed -i "s@security.ubuntu.com@mirrors.xxx.com@g" /etc/apt/sources.list \
    && sed -i "s@ports.ubuntu.com@mirrors.xxx.com@g" /etc/apt/sources.list
```

# centos
```
cp -a /etc/yum.repos.d/CentOS-Base.repo /etc/yum.repos.d/CentOS-Base.repo.bak
sed -i "s@mirror.centos.org@mirrors.xxx.com@g" /etc/yum.repos.d/CentOS-Base.repo
sed -i "s@#baseurl=@baseurl=@g" /etc/yum.repos.d/CentOS-Base.repo
sed -i "s@mirrorlist=@#mirrorlist=@g" /etc/yum.repos.d/CentOS-Base.repo
```

# alpine
```
cp /etc/apk/repositories /etc/apk/repositories.bak
sed -i "s@dl-cdn.alpinelinux.org/@mirrors.xxx.com/@g" /etc/apk/repositories
```

# rust
```
export RUSTUP_DIST_SERVER="https://mirrors.xxx.com/rust-static"
export RUSTUP_UPDATE_ROOT="https://mirrors.xxx.com/rust-static/rustup"
```
编辑文件 `~/.cargo/config` :
```
[source.crates-io]
replace-with = 'mirror'

[source.mirror]
registry = "sparse+https://mirrors.xxx.com/crates.io-index/"

[registries.mirror]
index = "sparse+https://mirrors.xxx.com/crates.io-index/"
```

# php composer
```
composer config -g repo.packagist composer https://mirrors.xxx.com/composer/

# 取消全局配置
composer config -g --unset repos.packagist
```

# pip
```
pip config set global.index-url https://mirrors.xxx.com/pypi/simple
```

# npm
```
echo "registry=https://mirrors.xxx.com/npm" > ~/.npmrc
```

# maven
```
<?xml version="1.0" encoding="UTF-8"?>
<settings xmlns="http://maven.apache.org/SETTINGS/1.0.0"
          xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
          xsi:schemaLocation="http://maven.apache.org/SETTINGS/1.0.0 http://maven.apache.org/xsd/settings-1.0.0.xsd">
  <mirrors>
    <mirror>
      <id>one-mirror</id>
      <mirrorOf>*</mirrorOf>
      <url>https://mirrors.xxx.com/maven</url>
    </mirror>
  </mirrors>
</settings>
```

# golang
```
export GOPROXY=https://mirrors.xxx.com/goproxy,direct
```

# docker image
编辑 `/etc/docker/daemon.json` :
```
{
  "registry-mirrors": [
    "https://mirrors.xxx.com"
  ]
}
```

