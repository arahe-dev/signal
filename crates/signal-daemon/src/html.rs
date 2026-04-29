use signal_core::models::{Message, MessageStatus, Reply};

pub fn render_inbox(messages: &[Message], token: Option<&str>) -> String {
    let token_param = token.map(|t| format!("?token={}", t)).unwrap_or_default();
    let mut html = String::new();
    html.push_str(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Signal Demo Inbox</title>
    <style>
        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #f5f5f7;
            color: #1d1d1f;
            line-height: 1.5;
            padding: 20px;
        }
        .container {
            max-width: 600px;
            margin: 0 auto;
        }
        h1 {
            font-size: 28px;
            font-weight: 600;
            margin-bottom: 24px;
            color: #1d1d1f;
        }
        .message-card {
            background: white;
            border-radius: 12px;
            padding: 16px;
            margin-bottom: 12px;
            box-shadow: 0 1px 3px rgba(0,0,0,0.1);
            transition: transform 0.2s, box-shadow 0.2s;
        }
        .message-card:hover {
            transform: translateY(-2px);
            box-shadow: 0 4px 12px rgba(0,0,0,0.15);
        }
        .message-card a {
            text-decoration: none;
            color: inherit;
            display: block;
        }
        .message-title {
            font-size: 17px;
            font-weight: 600;
            margin-bottom: 6px;
            color: #1d1d1f;
        }
        .message-body {
            font-size: 15px;
            color: #86868b;
            margin-bottom: 10px;
            display: -webkit-box;
            -webkit-line-clamp: 2;
            -webkit-box-orient: vertical;
            overflow: hidden;
        }
        .message-meta {
            display: flex;
            gap: 12px;
            font-size: 13px;
            color: #a1a1a6;
        }
        .meta-item {
            display: flex;
            align-items: center;
            gap: 4px;
        }
        .status-badge {
            display: inline-block;
            padding: 2px 8px;
            border-radius: 10px;
            font-size: 12px;
            font-weight: 500;
        }
        .status-new {
            background: #e3f2fd;
            color: #1976d2;
        }
        .status-pending {
            background: #fff3e0;
            color: #f57c00;
        }
        .status-consumed {
            background: #e8f5e9;
            color: #388e3c;
        }
        .status-archived {
            background: #f5f5f5;
            color: #757575;
        }
        .empty-state {
            text-align: center;
            padding: 60px 20px;
            color: #86868b;
        }
        .empty-state svg {
            width: 64px;
            height: 64px;
            margin-bottom: 16px;
            opacity: 0.5;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>Signal Demo Inbox</h1>
"#,
    );

    if messages.is_empty() {
        html.push_str(r#"<div class="empty-state">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"></path>
            </svg>
            <p>No messages yet</p>
            <p style="font-size: 14px; margin-top: 8px;">Send a message using the CLI to get started.</p>
        </div>"#);
    } else {
        for msg in messages {
            let status_class = match msg.status {
                MessageStatus::New => "status-new",
                MessageStatus::Pending => "status-pending",
                MessageStatus::Consumed => "status-consumed",
                MessageStatus::Archived => "status-archived",
            };
            let status_label = match msg.status {
                MessageStatus::New => "New",
                MessageStatus::Pending => "Pending",
                MessageStatus::Consumed => "Consumed",
                MessageStatus::Archived => "Archived",
            };
            let body_escaped = msg
                .body
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let title_escaped = msg
                .title
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");

            html.push_str(&format!(
                r#"<div class="message-card">
            <a href="/message/{}{}">
                <div class="message-title">{}</div>
                <div class="message-body">{}</div>
                <div class="message-meta">
                    <span class="meta-item">
                        <span class="status-badge {}">{}</span>
                    </span>
                    <span class="meta-item">{}</span>
                    {}
                </div>
            </a>
        </div>"#,
                msg.id,
                token_param,
                title_escaped,
                body_escaped,
                status_class,
                status_label,
                msg.source,
                msg.project
                    .as_ref()
                    .map(|p| format!("<span class=\"meta-item\">{}</span>", p))
                    .unwrap_or_default()
            ));
        }
    }

    html.push_str(
        r#"
    </div>
</body>
</html>"#,
    );

    html
}

pub fn render_message_detail(message: &Message, replies: &[Reply], token: Option<&str>) -> String {
    let mut html = String::new();
    let body_escaped = message
        .body
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let title_escaped = message
        .title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let token_field = token
        .map(|t| format!(r#"<input type="hidden" name="token" value="{}">"#, t))
        .unwrap_or_default();
    let status_class = match message.status {
        MessageStatus::New => "status-new",
        MessageStatus::Pending => "status-pending",
        MessageStatus::Consumed => "status-consumed",
        MessageStatus::Archived => "status-archived",
    };
    let status_label = match message.status {
        MessageStatus::New => "New",
        MessageStatus::Pending => "Pending",
        MessageStatus::Consumed => "Consumed",
        MessageStatus::Archived => "Archived",
    };

    html.push_str(&format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{} - Signal</title>
    <style>
        * {{
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #f5f5f7;
            color: #1d1d1f;
            line-height: 1.5;
            padding: 20px;
        }}
        .container {{
            max-width: 600px;
            margin: 0 auto;
        }}
        .back-link {{
            display: inline-flex;
            align-items: center;
            gap: 6px;
            color: #007aff;
            text-decoration: none;
            font-size: 15px;
            margin-bottom: 20px;
        }}
        .back-link:hover {{
            text-decoration: underline;
        }}
        .message-detail {{
            background: white;
            border-radius: 12px;
            padding: 20px;
            margin-bottom: 20px;
            box-shadow: 0 1px 3px rgba(0,0,0,0.1);
        }}
        .message-title {{
            font-size: 22px;
            font-weight: 600;
            margin-bottom: 12px;
        }}
        .message-body {{
            font-size: 16px;
            color: #424245;
            margin-bottom: 16px;
            white-space: pre-wrap;
        }}
        .message-meta {{
            display: flex;
            flex-wrap: wrap;
            gap: 12px;
            font-size: 13px;
            color: #86868b;
            padding-top: 12px;
            border-top: 1px solid #e5e5e7;
        }}
        .status-badge {{
            display: inline-block;
            padding: 2px 10px;
            border-radius: 10px;
            font-size: 12px;
            font-weight: 500;
        }}
        .status-new {{ background: #e3f2fd; color: #1976d2; }}
        .status-pending {{ background: #fff3e0; color: #f57c00; }}
        .status-consumed {{ background: #e8f5e9; color: #388e3c; }}
        .status-archived {{ background: #f5f5f5; color: #757575; }}
        .replies-section {{
            margin-bottom: 20px;
        }}
        .replies-title {{
            font-size: 18px;
            font-weight: 600;
            margin-bottom: 12px;
        }}
        .reply-card {{
            background: white;
            border-radius: 12px;
            padding: 16px;
            margin-bottom: 12px;
            box-shadow: 0 1px 3px rgba(0,0,0,0.1);
        }}
        .reply-body {{
            font-size: 15px;
            color: #424245;
            margin-bottom: 10px;
            white-space: pre-wrap;
        }}
        .reply-meta {{
            font-size: 13px;
            color: #a1a1a6;
        }}
        .reply-form {{
            background: white;
            border-radius: 12px;
            padding: 20px;
            box-shadow: 0 1px 3px rgba(0,0,0,0.1);
        }}
        .form-title {{
            font-size: 18px;
            font-weight: 600;
            margin-bottom: 16px;
        }}
        .form-group {{
            margin-bottom: 16px;
        }}
        .form-label {{
            display: block;
            font-size: 14px;
            font-weight: 500;
            margin-bottom: 6px;
            color: #424245;
        }}
        .form-textarea {{
            width: 100%;
            padding: 12px;
            border: 1px solid #d2d2d7;
            border-radius: 8px;
            font-size: 15px;
            font-family: inherit;
            resize: vertical;
            min-height: 100px;
        }}
        .form-textarea:focus {{
            outline: none;
            border-color: #007aff;
            box-shadow: 0 0 0 3px rgba(0,122,255,0.1);
        }}
        .form-submit {{
            background: #007aff;
            color: white;
            border: none;
            padding: 12px 24px;
            border-radius: 8px;
            font-size: 15px;
            font-weight: 500;
            cursor: pointer;
            transition: background 0.2s;
        }}
        .form-submit:hover {{
            background: #0056b3;
        }}
        .no-replies {{
            color: #86868b;
            font-size: 14px;
            margin-bottom: 20px;
        }}
    </style>
</head>
<body>
    <div class="container">
        <a href="/" class="back-link">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M19 12H5M12 19l-7-7 7-7"/>
            </svg>
            Back to Inbox
        </a>

        <div class="message-detail">
            <h1 class="message-title">{}</h1>
            <div class="message-body">{}</div>
            <div class="message-meta">
                <span class="status-badge {}">{}</span>
                <span>From: {}</span>
                {}
                {}
            </div>
        </div>

        <div class="replies-section">
            <h2 class="replies-title">Replies</h2>
"#,
        title_escaped,
        title_escaped,
        body_escaped,
        status_class,
        status_label,
        message.source,
        message.project.as_ref().map(|p| format!("<span>Project: {}</span>", p)).unwrap_or_default(),
        message.agent_id.as_ref().map(|a| format!("<span>Agent: {}</span>", a)).unwrap_or_default()
    ));

    if replies.is_empty() {
        html.push_str("<p class=\"no-replies\">No replies yet.</p>");
    } else {
        for reply in replies {
            let reply_body_escaped = reply
                .body
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let created_at = reply.created_at.format("%b %d, %Y at %H:%M").to_string();
            html.push_str(&format!(
                r#"<div class="reply-card">
                <div class="reply-body">{}</div>
                <div class="reply-meta">From: {} - {}</div>
            </div>"#,
                reply_body_escaped, reply.source, created_at
            ));
        }
    }

    html.push_str(&format!(r#"
        </div>

        <div class="reply-form">
            <h3 class="form-title">Add Reply</h3>
            <form method="POST" action="/api/messages/{}/replies/form">
                {}
                <div class="form-group">
                    <label class="form-label" for="body">Your reply</label>
                    <textarea class="form-textarea" id="body" name="body" required placeholder="Type your reply here..."></textarea>
                </div>
                <button type="submit" class="form-submit">Send Reply</button>
            </form>
        </div>
    </div>
</body>
</html>"#, message.id, token_field));

    html
}
