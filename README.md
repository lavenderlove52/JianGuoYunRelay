# JianGuoYunRelay
# 坚果云服务器中转
# 部署
1.在云服务器创建服务文件：
```shell
sudo vi /etc/systemd/system/jian-guo-yun-relay.service
```

写入下面内容：
```shell
[Unit]
Description=JianGuoYunRelay
After=network.target
[Service]
Type=simple
WorkingDirectory=/opt/JianGuoYunRelay
ExecStart=/opt/JianGuoYunRelay/target/release/jian_guo_yun_relay
Restart=always
RestartSec=3
User=root
Environment=RUST_LOG=info
[Install]
WantedBy=multi-user.target
```
注意两点：
```shell
WorkingDirectory=/opt/JianGuoYunRelay 很重要，因为程序会从这里读取 .env
你现在监听 10000 端口，不是特权端口，所以后面也可以改成普通用户运行
```


启动命令
```shell
sudo systemctl daemon-reload
sudo systemctl enable jian-guo-yun-relay
sudo systemctl start jian-guo-yun-relay
```

查看状态：
```shell
sudo systemctl status jian-guo-yun-relay
```

查看日志：
```shell
sudo journalctl -u jian-guo-yun-relay -f
```

以后更新怎么做


```shell
cd /opt/JianGuoYunRelay
cargo build --release
sudo systemctl restart jian-guo-yun-relay
```

# 反向代理
## 1. 先把中继只监听本机
建议把云服务器 .env 里的：
LISTEN_ADDR=127.0.0.1:xxxxx
这样外网不能直接打到 Rust 服务，只能经过 nginx。
改完后重启中继服务。
## 2. 安装 nginx
   CentOS 一般是：
```shell
sudo yum install -y nginx
```

启动并开机自启：
```shell
sudo systemctl enable nginx
sudo systemctl start nginx
```

## 3. 配置反向代理
创建配置文件：
```shell
sudo vi /etc/nginx/conf.d/jian-guo-yun-relay.conf
```

写入：
```shell
server {
listen 80;
server_name 124.223.116.8;
client_max_body_size 50m;
location / {
proxy_pass http://127.0.0.1:10000;
proxy_http_version 1.1;
proxy_set_header Host $host;
proxy_set_header X-Real-IP $remote_addr;
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
proxy_set_header X-Forwarded-Proto $scheme;
# 很重要，确保 Basic/Bearer 头透传
proxy_set_header Authorization $http_authorization;
# WebDAV / KeePass 场景建议关闭缓冲
proxy_request_buffering off;
proxy_buffering off;
}
}
```
如果你以后用域名，把 server_name 改成你的域名。

## 4. 检查并重载 nginx
```shell
sudo nginx -t
sudo systemctl reload nginx
```
