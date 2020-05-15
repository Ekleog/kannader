#![allow(unused_must_use)]

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use smtp_message::Email;

pub fn parse_email(c: &mut Criterion) {
    let tests: &[&[&[u8]]] = &[
        &[b"postmaster", b"test", b"foobar", b"root"],
        &[
            b"t+e-s.t_i+n-g@foo.bar.baz",
            b"hello@world.com",
            b"this.is.a.very.long.message@lists.subdomain.domain.tld",
        ],
        &[
            br#""quoted\"example"@example.org"#,
            br#""and with\@stuff like this\"\\"@test.com"#,
        ],
    ];
    let names: &[&str] = &["localpart only", "normal email", "quoted-string localpart"];

    let mut g = c.benchmark_group("Email::parse");
    // https://github.com/38/plotters/issues/143
    // g.plot_config(PlotConfiguration::default().summary_scale(AxisScale::
    // Logarithmic));

    for i in 0..tests.len() {
        let n = names[i];
        g.throughput(Throughput::Bytes(
            tests[i].iter().map(|s| s.len() as u64).sum(),
        ));
        // https://github.com/bheisler/criterion.rs/issues/382
        // g.throughput(Throughput::Elements(tests[i].len() as u64));
        g.bench_with_input(BenchmarkId::new("smtp-message", n), tests[i], |b, tests| {
            b.iter(|| {
                for t in tests {
                    black_box(Email::<&str>::parse(t));
                }
            })
        });
        g.bench_with_input(
            BenchmarkId::new("smtp-message-alloc", n),
            tests[i],
            |b, tests| {
                b.iter(|| {
                    for t in tests {
                        black_box(Email::<String>::parse(t));
                    }
                })
            },
        );
        g.bench_with_input(
            BenchmarkId::new("rustyknife-legacy", n),
            tests[i],
            |b, tests| {
                b.iter(|| {
                    for t in tests {
                        black_box(rustyknife::rfc5321::validate_address::<
                            rustyknife::behaviour::Legacy,
                        >(t));
                    }
                })
            },
        );
        g.bench_with_input(
            BenchmarkId::new("rustyknife-intl", n),
            tests[i],
            |b, tests| {
                b.iter(|| {
                    for t in tests {
                        black_box(rustyknife::rfc5321::validate_address::<
                            rustyknife::behaviour::Intl,
                        >(t));
                    }
                })
            },
        );
    }
}

criterion_group!(benches, parse_email);
criterion_main!(benches);
