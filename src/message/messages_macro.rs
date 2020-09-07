/// Macro for messages' description
#[macro_export]
macro_rules! messages {
    (
        $(
            $(#[derive($derives:meta)])*
            #[cli(
                name = $cli_name:literal
                $(, about = $about:literal)?
                $(, version = $version:literal)?)
            ]
            message $name:ident {
                $($tt:tt)*
            }
        )+

        $(
            fwd_messages {
                $(
                    message $fwd_name:ident($($fwd_tt:tt)*);
                ),+
            }
        )?

        $(
            submessages {
                $(
                    $submsg_name:ident($submsg_path:path)
                ),+
            }
        )?
    ) => {
        messages![@_impl cli_messages $($cli_name) +];

        #[derive(Debug)]
        pub enum Message {
            $($name($name)),+
            $(
                , $($fwd_name($($fwd_tt)*)),+
            )?
            $(
                , $($submsg_name($submsg_path)),+
            )?
        }

        impl Message {
            /// Returns CLI message list
            ///
            /// Format: (name, optional about)
            pub fn cli_list() -> &'static Vec<(&'static str, Option<&'static str>)> {
                use lazy_static::lazy_static;

                lazy_static! {
                    static ref LIST: Vec<(&'static str, Option<&'static str>)> = {
                        #[allow(unused_mut)]
                        let mut init_list = messages![@_impl cli_list
                            $(
                                ($cli_name, $($about)?)
                            ) +
                        ];

                        $(
                            use std::iter::Extend;

                            $(
                                init_list.extend(<$submsg_path>::cli_list());
                            )+
                        )?

                        init_list
                    };
                }

                &LIST
            }

            pub fn from_vec(args: &Vec<&str>) -> $crate::Result<Self> {
                #[cfg(debug_assertions)]
                {
                    use lazy_static::lazy_static;
                    use std::{
                        sync::Once,
                        collections::HashSet
                    };

                    lazy_static! {
                        static ref DUB_CHECK: Once = Once::new();
                    }

                    DUB_CHECK.call_once(|| {
                        let mut unique = HashSet::new();

                        let all_message_are_unique = Message::cli_list().iter()
                            .map(|(name, _)| name)
                            .all(move |name| {
                                let is_unique = unique.insert(name);
                                if !is_unique {
                                    println!(">>> MESSAGE DUBLICATE FOUND: {}", name);
                                }

                                is_unique
                            });

                        if !all_message_are_unique {
                            panic!("FOUND DUBLICATE MESSAGES");
                        }
                    });
                }

                if args.is_empty() {
                    return Err($crate::Error::MissingMessage);
                }

                let message_name = args[0];

                match message_name {
                    $(
                        $cli_name => {
                            let message = $name::from_vec(args)?;
                            return Ok(Message::$name(message));
                        }
                    )+
                    _ => {}
                }

                messages! {
                    @_impl from_vec(args) for submessages($($($submsg_name($submsg_path)),+)?)
                    else {
                        Err($crate::Error::UnknownMessage(message_name.into()))
                    }
                }
            }

            pub fn cli_autocomplete<T: From<&'static str>>(input_msg: &str) -> Vec<T> {
                use std::iter::Extend;

                let mut result = vec![];

                $(
                    $(
                        let mut sub_msg_result = <$submsg_path>::cli_autocomplete(input_msg);
                        result.append(&mut sub_msg_result);
                    )+
                )?

                result.extend(
                    CLI_MESSAGES.iter()
                        .filter(|msg| msg.starts_with(input_msg))
                        .map(|msg| (*msg).into())
                );

                result
            }

            pub fn get_cli_name(&self) -> &'static str {
                match self {
                    $(Message::$name(_) => <$name>::get_cli_name()),+
                    $(
                        , $(Message::$fwd_name(_) => concat!("/fwd message: \"", stringify![$fwd_name], "\"/")),+
                    )?
                    $(
                        , $(Message::$submsg_name(msg) => msg.get_cli_name()),+
                    )?,
                }
            }
        }

        impl std::str::FromStr for Message {
            type Err = $crate::Error;

            fn from_str(s: &str) -> $crate::Result<Self> {
                let args = s.split(" ").collect::<Vec<_>>();
                Self::from_vec(&args)
            }
        }

        $(
            #[derive(Debug, $($derives)*)]
            #[derive(structopt::StructOpt)]
            #[structopt(name = $cli_name $(, about = $about)? $(, version = $version)?)]
            pub struct $name {
                $($tt)*
            }

            impl $name {
                pub fn from_vec(args: &Vec<&str>) -> $crate::Result<Self> {
                    let _unused = $cli_name;
                    let iter = args.iter();

                    structopt::StructOpt::from_iter_safe(iter)
                        .map_err(|err| err.into())
                }

                pub fn get_cli_name() -> &'static str {
                    $cli_name
                }
            }

            impl From<$name> for Message {
                fn from(message: $name) -> Self {
                    Message::$name(message)
                }
            }

            impl std::str::FromStr for $name {
                type Err = $crate::Error;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    let _unused = $cli_name;
                    let args = s.split(" ").collect::<Vec<_>>();
                    Self::from_vec(&args)
                }
            }
        )+

        $(
            $(
                impl From<$submsg_path> for Message {
                    fn from(message: $submsg_path) -> Self {
                        Message::$submsg_name(message)
                    }
                }
            )+
        )?
    };

    (@_impl optional) => (None);
    (@_impl optional $cli_name:literal) => (Some($cli_name));

    (@_impl cli_messages $($cli_name:literal) +) => {
        static CLI_MESSAGES: &[&'static str] = &[
            $($cli_name),+
        ];
    };

    (@_impl cli_list $(($cli_name:literal, $($about:literal)?)) *) => {
        vec![
            $(($cli_name, messages![@_impl optional $($about)?])),*
        ]
    };

    (@_impl from_vec($args:expr) for submessages($($submsg_name:ident($submsg_path:path)),*)
        else $else_block:block
    ) => {
        $(match <$submsg_path>::from_vec($args) {
            Ok(msg) => return Ok(Message::$submsg_name(msg)),
            Err($crate::Error::UnknownMessage(_)) => {},
            Err(err) => return Err(err)
        })*

        #[allow(unused_braces)]
        $else_block
    };

    (@_impl from_vec($args:expr) for submessages()
        else $else_block:block
    ) => {
        $else_block
    };
}
