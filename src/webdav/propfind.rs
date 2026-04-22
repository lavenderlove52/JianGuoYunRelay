//! 最小 PROPFIND 多状态响应（RFC 4918），供 KeePass 等探测。

/// `href` 应为对外可见路径（如 `/vault.kdbx`）。
pub fn propfind_multistatus(href: &str, display_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>{href}</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype/>
        <D:getcontenttype>application/octet-stream</D:getcontenttype>
        <D:displayname>{display_name}</D:displayname>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#
    )
}
