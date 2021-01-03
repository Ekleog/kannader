use smtp_message::{EnhancedReplyCode, MaybeUtf8, Reply, ReplyCode};

#[inline]
pub fn welcome_banner(hostname: &str, banner: &str) -> Reply<String> {
    Reply {
        code: ReplyCode::SERVICE_READY,
        ecode: None,
        text: vec![MaybeUtf8::Utf8(String::from(hostname) + " " + banner)],
    }
}

/// Usual value for returning “Okay” from `filter_hello`
#[inline]
pub fn okay_hello(
    is_ehlo: bool,
    local_hostname: &str,
    banner: &str,
    can_do_tls: bool,
) -> Reply<String> {
    let mut built_banner = String::from(local_hostname);
    if !banner.is_empty() {
        built_banner += " ";
        built_banner += banner;
    }
    let mut text = vec![MaybeUtf8::Utf8(built_banner)];
    if is_ehlo {
        text.push(MaybeUtf8::Ascii("8BITMIME".into()));
        text.push(MaybeUtf8::Ascii("ENHANCEDSTATUSCODES".into()));
        text.push(MaybeUtf8::Ascii("PIPELINING".into()));
        text.push(MaybeUtf8::Ascii("SMTPUTF8".into()));
        if can_do_tls {
            text.push(MaybeUtf8::Ascii("STARTTLS".into()));
        }
    }
    Reply {
        code: ReplyCode::OKAY,
        ecode: None,
        text,
    }
}

#[inline]
pub fn okay(ecode: EnhancedReplyCode<&'static str>) -> Reply<&'static str> {
    Reply {
        code: ReplyCode::OKAY,
        ecode: Some(ecode),
        text: vec![MaybeUtf8::Ascii("Okay")],
    }
}

/// Usual value for returning “Okay” from `filter_from`
#[inline]
pub fn okay_from() -> Reply<&'static str> {
    okay(EnhancedReplyCode::SUCCESS_UNDEFINED)
}

/// Usual value for returning “Okay” from `filter_to`
#[inline]
pub fn okay_to() -> Reply<&'static str> {
    okay(EnhancedReplyCode::SUCCESS_DEST_VALID)
}

/// Usual value for returning “Okay” from `filter_data`
#[inline]
pub fn okay_data() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::START_MAIL_INPUT,
        ecode: None,
        text: vec![MaybeUtf8::Ascii("Start mail input; end with <CRLF>.<CRLF>")],
    }
}

/// Usual value for returning “Okay” from `handle_mail`
#[inline]
pub fn okay_mail() -> Reply<&'static str> {
    okay(EnhancedReplyCode::SUCCESS_UNDEFINED)
}

/// Usual value for returning “Okay” from `handle_starttls`
#[inline]
pub fn okay_starttls() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::SERVICE_READY,
        ecode: Some(EnhancedReplyCode::SUCCESS_UNDEFINED),
        text: vec![MaybeUtf8::Ascii("Ready to start TLS")],
    }
}

/// Usual value for returning “Okay” from `handle_rset`
#[inline]
pub fn okay_rset() -> Reply<&'static str> {
    okay(EnhancedReplyCode::SUCCESS_UNDEFINED)
}

/// Usual value for ignoring the request but returning “Okay” from `handle_vrfy`
#[inline]
pub fn ignore_vrfy() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::CANNOT_VRFY_BUT_PLEASE_TRY,
        ecode: Some(EnhancedReplyCode::SUCCESS_DEST_VALID),
        text: vec![MaybeUtf8::Ascii(
            "Cannot VRFY user, but will accept message and attempt delivery",
        )],
    }
}

/// Usual value for ignoring the request but returning a generic message from
/// `handle_help`
#[inline]
pub fn ignore_help() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::HELP_MESSAGE,
        ecode: Some(EnhancedReplyCode::SUCCESS_UNDEFINED),
        text: vec![MaybeUtf8::Ascii("See https://tools.ietf.org/html/rfc5321")],
    }
}

/// Usual value for returning “Okay” from `handle_noop`
#[inline]
pub fn okay_noop() -> Reply<&'static str> {
    okay(EnhancedReplyCode::SUCCESS_UNDEFINED)
}

/// Usual value for returning “Okay” from `handle_quit`
#[inline]
pub fn okay_quit() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::CLOSING_CHANNEL,
        ecode: Some(EnhancedReplyCode::SUCCESS_UNDEFINED),
        text: vec![MaybeUtf8::Utf8("Bye")],
    }
}

/// Usual value for returning “Okay” from `already_did_hello`,
/// `mail_before_hello`, `already_in_mail`, `rcpt_before_mail`,
/// `data_before_rcpt` and `data_before_mail`
#[inline]
pub fn bad_sequence() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::BAD_SEQUENCE,
        ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND),
        text: vec![MaybeUtf8::Ascii("Bad sequence of commands")],
    }
}

#[inline]
pub fn command_unimplemented() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::COMMAND_UNIMPLEMENTED,
        ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND),
        text: vec![MaybeUtf8::Ascii("Command not implemented")],
    }
}

#[inline]
pub fn command_unrecognized() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::COMMAND_UNRECOGNIZED,
        ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND),
        text: vec![MaybeUtf8::Ascii("Command not recognized")],
    }
}

#[inline]
pub fn command_not_supported() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::COMMAND_UNIMPLEMENTED,
        ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND),
        text: vec![MaybeUtf8::Ascii("Command not supported")],
    }
}

#[inline]
pub fn pipeline_forbidden_after_starttls() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::BAD_SEQUENCE,
        ecode: Some(EnhancedReplyCode::PERMANENT_INVALID_COMMAND),
        text: vec![MaybeUtf8::Ascii("Pipelining after starttls is forbidden")],
    }
}

#[inline]
pub fn line_too_long() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::COMMAND_UNRECOGNIZED,
        ecode: Some(EnhancedReplyCode::PERMANENT_UNDEFINED),
        text: vec![MaybeUtf8::Ascii("Line too long")],
    }
}

#[inline]
pub fn internal_server_error() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::LOCAL_ERROR,
        ecode: Some(EnhancedReplyCode::TRANSIENT_UNDEFINED),
        text: vec![MaybeUtf8::Ascii("Internal server error")],
    }
}

#[inline]
pub fn handle_mail_did_not_call_complete() -> Reply<&'static str> {
    Reply {
        code: ReplyCode::LOCAL_ERROR,
        ecode: Some(EnhancedReplyCode::TRANSIENT_SYSTEM_INCORRECTLY_CONFIGURED),
        text: vec![MaybeUtf8::Ascii("System incorrectly configured")],
    }
}
