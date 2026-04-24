//! 最小 PROPFIND 多状态响应（RFC 4918），供 KeePass 等探测。

/// `href` 应为对外可见路径（如 `/vault.kdbx`）。
pub fn propfind_multistatus(
    href: &str,
    display_name: &str,
    content_length: Option<&str>,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> String {
    let content_length = content_length.unwrap_or("0");
    let etag = etag.unwrap_or("");
    let last_modified = last_modified.unwrap_or("");
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>{href}</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype/>
        <D:getcontenttype>application/octet-stream</D:getcontenttype>
        <D:getcontentlength>{content_length}</D:getcontentlength>
        <D:getetag>{etag}</D:getetag>
        <D:getlastmodified>{last_modified}</D:getlastmodified>
        <D:displayname>{display_name}</D:displayname>
      </D:prop>
      <D:status>HTTP/1.1 200 OK</D:status>
    </D:propstat>
  </D:response>
</D:multistatus>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::propfind_multistatus;

    #[test]
    fn includes_common_file_metadata_fields() {
        let xml = propfind_multistatus(
            "/KeePass/数据库.kdbx",
            "数据库.kdbx",
            Some("24439"),
            Some("\"etag-1\""),
            Some("Fri, 24 Apr 2026 06:00:50 GMT"),
        );

        assert!(xml.contains("<D:getcontenttype>application/octet-stream</D:getcontenttype>"));
        assert!(xml.contains("<D:getcontentlength>24439</D:getcontentlength>"));
        assert!(xml.contains("<D:getetag>\"etag-1\"</D:getetag>"));
        assert!(xml.contains("<D:getlastmodified>Fri, 24 Apr 2026 06:00:50 GMT</D:getlastmodified>"));
    }
}
