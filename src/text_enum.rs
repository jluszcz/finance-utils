//! [`text_enum!`](crate::text_enum).

/// The three things a Rust enum behind a `TEXT` column needs: the list of
/// variants (`ALL`), the token each is stored as (`as_str`), and reading a
/// token back (`FromStr`). There is also `index`, a variant's place in `ALL`.
///
/// One list rather than three, so adding a variant to two of the three cannot
/// happen: `from_str` searches `ALL` through `as_str`. `$what` is the noun the
/// refusal names (`unknown account kind "x"`), so an unreadable column says
/// which column it was. A display `label` is deliberately not generated: it is
/// prose free to change without a migration, where `as_str` is pinned by the
/// schema. The enum must derive `Copy` and `PartialEq`.
#[macro_export]
macro_rules! text_enum {
    (
        $name:ident, $what:literal,
        $(#[$all:meta])*
        [$($variant:ident => $token:literal),+ $(,)?]
    ) => {
        impl $name {
            $(#[$all])*
            pub const ALL: [$name; [$($name::$variant),+].len()] = [$($name::$variant),+];

            /// The token this variant is stored as, and the one the schema's
            /// `CHECK` names. Changing one is a migration.
            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $token,)+
                }
            }

            /// This variant's place in [`Self::ALL`].
            pub fn index(self) -> usize {
                $name::ALL
                    .iter()
                    .position(|variant| *variant == self)
                    .expect(concat!(stringify!($name), "::ALL names every variant"))
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::__anyhow::Error;

            fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> {
                $name::ALL
                    .into_iter()
                    .find(|variant| variant.as_str() == s)
                    .ok_or_else(|| {
                        $crate::__anyhow::anyhow!(concat!("unknown ", $what, " {:?}"), s)
                    })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    enum Color {
        Red,
        Green,
    }

    crate::text_enum!(Color, "color", [Red => "red", Green => "green"]);

    #[test]
    fn every_variant_round_trips_through_its_token() {
        for color in Color::ALL {
            assert_eq!(color.as_str().parse::<Color>().unwrap(), color);
        }
    }

    #[test]
    fn all_lists_the_variants_in_declared_order_and_index_reads_it() {
        assert_eq!(Color::ALL, [Color::Red, Color::Green]);
        assert_eq!(Color::Green.index(), 1);
    }

    #[test]
    fn an_unknown_token_is_refused_naming_what_it_was_read_as() {
        let err = "blue".parse::<Color>().unwrap_err().to_string();
        assert_eq!(err, "unknown color \"blue\"");
    }
}
