//! Real codec and source-identity tests without provider requests.
//! 不请求供应商的真实编解码及来源身份测试。

use super::*;

/// Produces one real RGBA image encoded as `format`; returns portable fixture bytes.
/// 生成按`format`编码的真实RGBA图片；返回可移植夹具字节。
fn fixture_bytes(format: ImageFormat) -> Vec<u8> {
    // Unequal dimensions and transparency make accidental blank output observable.
    // 不同宽高及透明度使意外空白输出可被观测。
    let pixels = image::RgbaImage::from_pixel(2, 3, image::Rgba([24, 80, 160, 200]));
    // JPEG lacks alpha and is explicitly encoded through its supported RGB representation.
    // JPEG没有透明通道，显式使用其支持的RGB表示编码。
    let image = if format == ImageFormat::Jpeg {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(pixels).to_rgb8())
    } else {
        DynamicImage::ImageRgba8(pixels)
    };
    // Fixture bytes are independent of the production conversion entry point.
    // 夹具字节独立于生产转换入口。
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}

/// Every enabled static codec produces actual PNG pixels while preserving dimensions and nonlossy alpha.
/// 每个启用的静态编解码器均产出真实PNG像素，并保留尺寸及无损透明度。
#[test]
fn image_conversion_static_codecs_produce_png_pixels() {
    for format in [
        ImageFormat::Jpeg,
        ImageFormat::Bmp,
        ImageFormat::Gif,
        ImageFormat::WebP,
        ImageFormat::Tiff,
        ImageFormat::Ico,
        ImageFormat::Tga,
        ImageFormat::Qoi,
    ] {
        // Encode a real source rather than accepting a renamed arbitrary byte fixture.
        // 编码真实来源，不使用仅改名的任意字节夹具。
        let source = fixture_bytes(format);
        // The production converter owns the temporary output throughout PNG verification.
        // 生产转换器在PNG验证期间持有临时输出。
        let file =
            encode_png(&source, format).unwrap_or_else(|error| panic!("{format:?}: {error}"));
        // Decode with the explicit PNG codec to verify content, not merely the filename.
        // 使用明确PNG编解码器解码以验证内容，而不仅是文件名。
        let png = image::load_from_memory_with_format(
            &std::fs::read(file.path()).unwrap(),
            ImageFormat::Png,
        )
        .unwrap();
        assert_eq!((png.width(), png.height()), (2, 3), "{format:?}");
        if !matches!(format, ImageFormat::Jpeg | ImageFormat::Gif) {
            assert_eq!(
                png.to_rgba8().get_pixel(0, 0).0,
                [24, 80, 160, 200],
                "{format:?}"
            );
        }
    }
    for bytes in [
        b"P1\n2 1\n0 1\n".as_slice(),
        b"P2\n2 1\n255\n0 255\n",
        b"P3\n2 1\n255\n0 0 0 255 255 255\n",
        b"P7\nWIDTH 2\nHEIGHT 1\nDEPTH 1\nMAXVAL 255\nTUPLTYPE GRAYSCALE\nENDHDR\n\0\xff",
    ] {
        // Each PNM family advertised by the registry must pass its real header and pixel decoder.
        // 注册表声明的每种PNM族都必须通过真实头部及像素解码器。
        let file = encode_png(bytes, ImageFormat::Pnm).unwrap();
        assert!(
            std::fs::read(file.path())
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );
    }
}

/// Animation, multiple pages and corrupt sources fail instead of silently yielding partial content.
/// 动画、多页及损坏来源直接失败，不静默产出部分内容。
#[test]
fn image_conversion_rejects_lossy_container_collapse_and_corruption() {
    // Two real GIF frames prove the converter does not simply decode the first image.
    // 两个真实GIF帧证明转换器不只是解码第一张图。
    let mut gif = Vec::new();
    {
        // Encoder writes a valid multi-frame animation into the fixture buffer.
        // 编码器向夹具缓冲区写入有效多帧动画。
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif);
        for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    1,
                    1,
                    image::Rgba(color),
                )))
                .unwrap();
        }
    }
    assert!(
        encode_png(&gif, ImageFormat::Gif)
            .unwrap_err()
            .contains("animated GIF")
    );
    // TIFF encoder appends two distinct IFD pages without inventing binary offsets in the test.
    // TIFF编码器追加两个独立IFD页，测试不虚构二进制偏移。
    let mut tiff_bytes = Cursor::new(Vec::new());
    {
        // Both pages are valid and independently decodable.
        // 两页均有效且可独立解码。
        let mut encoder = tiff::encoder::TiffEncoder::new(&mut tiff_bytes).unwrap();
        encoder
            .write_image::<tiff::encoder::colortype::RGB8>(1, 1, &[1, 2, 3])
            .unwrap();
        encoder
            .write_image::<tiff::encoder::colortype::RGB8>(1, 1, &[4, 5, 6])
            .unwrap();
    }
    assert!(
        encode_png(tiff_bytes.get_ref(), ImageFormat::Tiff)
            .unwrap_err()
            .contains("multipage TIFF")
    );
    assert!(encode_png(b"not an image", ImageFormat::Bmp).is_err());
}

/// Registered identity and target PNG admission gate both replacement-model and composer conversion.
/// 注册身份及目标PNG准入共同门控替代模型和输入区转换。
#[tokio::test]
async fn image_conversion_preserves_original_and_checks_source_and_destination() {
    // Isolated storage contains the original image only; conversion creates a distinct temporary file.
    // 隔离存储仅包含原图；转换创建独立临时文件。
    let directory = tempfile::tempdir().unwrap();
    // TIFF exercises an image outside the default native format list.
    // TIFF覆盖默认原生格式列表之外的图片。
    let source = directory.path().join("source.tiff");
    std::fs::write(&source, fixture_bytes(ImageFormat::Tiff)).unwrap();
    // Canonical original proof is the same metadata carried by staged attachments.
    // 规范原始证明与暂存附件携带的元数据相同。
    let attachment = ConversationAttachment {
        kind: ConversationAttachmentKind::Image,
        file_name: "source.tiff".into(),
        local_path: source.to_string_lossy().into_owned(),
        mime_type: "image/tiff".into(),
        byte_len: Some(std::fs::metadata(&source).unwrap().len()),
        source_sha256: Some(crate::attachment_assets::streaming_file_sha256(&source).unwrap()),
        asset_id: Some("original-asset".into()),
        resource_path: Some("attachment://original-asset".into()),
        source_path: None,
        delivery_mode: None,
        preview_local_path: None,
    };
    assert!(
        convert_attachment_if_needed(&attachment, true, false)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        convert_attachment_if_needed(&attachment, false, false)
            .await
            .err()
            .unwrap()
            .contains("does not accept image/png")
    );
    // Successful conversion must clear the original content identity and preserve its source bytes.
    // 成功转换必须清除原内容身份，同时保留其来源字节。
    let converted = convert_attachment_if_needed(&attachment, false, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(converted.attachment.mime_type, "image/png");
    assert_eq!(converted.attachment.file_name, "source.png");
    assert_eq!(converted.attachment.asset_id, None);
    assert_eq!(
        crate::attachment_assets::streaming_file_sha256(&source).unwrap(),
        attachment.source_sha256.as_ref().unwrap().as_str()
    );
    // Altering canonical proof is rejected before any source decoding.
    // 更改规范证明会在任何来源解码之前被拒绝。
    let mut changed = attachment;
    changed.source_sha256 = Some("wrong".into());
    assert!(
        convert_attachment_if_needed(&changed, false, true)
            .await
            .err()
            .unwrap()
            .contains("registered identity")
    );
}

/// Provider-operation understanding converts against its own profile and keeps the owned locator association.
/// 供应商操作理解按自身Profile转换，并保留有归属定位符关联。
#[tokio::test]
async fn image_conversion_provider_understanding_rewrites_verified_source_only() {
    // The direct operation accepts PNG only, independent of any primary conversation model.
    // 直连操作仅接受PNG，独立于任何主会话模型。
    let profile = vulcan_provider_adapters::ProviderToolMediaInputProfile {
        resource_kind: vulcan_ai_contract::ResourceKind::Image,
        routes: vec![
            vulcan_provider_adapters::ProviderToolMediaInputRouteConstraint {
                upload_channel_id: "fixture".into(),
                accepted_mime_types: vec!["image/png".into()],
                max_bytes: None,
                max_count: Some(1),
            },
        ],
    };
    // Source file carries an authoritative digest, as in the real owned resource resolver.
    // 来源文件携带权威摘要，与真实有归属资源解析器一致。
    let directory = tempfile::tempdir().unwrap();
    // BMP is rejected by this fixture's PNG-only operation contract.
    // 此夹具的仅PNG操作契约拒绝BMP。
    let path = directory.path().join("picture.bmp");
    std::fs::write(&path, fixture_bytes(ImageFormat::Bmp)).unwrap();
    // Source proof has the same closed shape used by direct provider dispatch.
    // 来源证明具有直连供应商分派使用的相同封闭形状。
    let mut sources = vec![vulcan_provider_adapters::ProviderToolMediaSource {
        reference: "attachment://original".into(),
        local_path: path.clone(),
        file_name: "picture.bmp".into(),
        mime_type: "image/bmp".into(),
        source_identity: PreparedMediaSourceIdentity {
            byte_len: std::fs::metadata(&path).unwrap().len(),
            content_sha256: crate::attachment_assets::streaming_file_sha256(&path).unwrap(),
        },
    }];
    // Keep returned owners alive while validating the rewritten upload source.
    // 验证改写的上传来源期间保持返回所有者存活。
    let owners = convert_provider_sources(&mut sources, &profile)
        .await
        .unwrap();
    assert_eq!(owners.len(), 1);
    // Exactly one media-understanding source belongs to this fixture.
    // 此夹具恰好持有一个媒体理解来源。
    let source = sources.first().unwrap();
    assert_eq!(source.reference, "attachment://original");
    assert_eq!(source.mime_type, "image/png");
    assert!(
        std::fs::read(&source.local_path)
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    assert_eq!(
        source.source_identity.content_sha256,
        crate::attachment_assets::streaming_file_sha256(&source.local_path).unwrap()
    );
    assert!(path.exists());
}
