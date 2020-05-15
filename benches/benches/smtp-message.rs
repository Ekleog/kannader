use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use smtp_message::Email;

pub fn parse_email(c: &mut Criterion) {
    let tests: &[&[u8]] = &[
        b"postmaster",
        b"t+e-s.t_i+n-g@foo.bar.baz",
        br#""quoted\"example"@example.org"#,
    ];
    let names: &[&str] = &["localpart only", "normal email", "quoted-string localpart"];
    let mut g = c.benchmark_group("Email::parse");
    for i in 0..tests.len() {
        let n = names[i];
        g.bench_with_input(BenchmarkId::new("smtp-message", n), &i, |b, i| {
            b.iter(|| Email::<&str>::parse(tests[*i]))
        });
        g.bench_with_input(BenchmarkId::new("smtp-message-alloc", n), &i, |b, i| {
            b.iter(|| Email::<String>::parse(tests[*i]))
        });
        g.bench_with_input(BenchmarkId::new("rustyknife-legacy", n), &i, |b, i| {
            b.iter(|| {
                rustyknife::rfc5321::validate_address::<rustyknife::behaviour::Legacy>(tests[*i])
            })
        });
        g.bench_with_input(BenchmarkId::new("rustyknife-intl", n), &i, |b, i| {
            b.iter(|| {
                rustyknife::rfc5321::validate_address::<rustyknife::behaviour::Intl>(tests[*i])
            })
        });
    }
}

criterion_group!(benches, parse_email);
criterion_main!(benches);
