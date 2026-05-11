

fn add_subtitles(input_path: &PathBuf, subtitle_path: &str, output_path: &PathBuf) -> Result<(), String> {
    info!("添加字幕: subtitle_path={}", subtitle_path);
    
    let input_str = input_path.to_string_lossy().to_string();
    let output_str = output_path.to_string_lossy().to_string();
    
    let encoder = detect_best_encoder();
    
    let temp_dir = std::env::temp_dir();
    let temp_subtitle_path = temp_dir.join("temp_subtitle.srt");
    
    let subtitle_to_use = if let Ok(_) = std::fs::copy(subtitle_path, &temp_subtitle_path) {
        info!("已复制字幕文件到临时位置: {:?}", temp_subtitle_path);
        temp_subtitle_path.to_string_lossy().to_string()
    } else {
        subtitle_path.to_string()
    };
    
    let mut succeeded = false;
    
    // 方法1: 使用 ass 滤镜，双引号包裹路径（标准语法）
    {
        let filter_str = format!("ass=\"{}\"", &subtitle_to_use);
        info!("尝试方法1 (ass 双引号): {}", filter_str);
        
        let mut args: Vec<&str> = vec![
            "-hide_banner",
            "-loglevel", "error",
            "-i", &input_str,
            "-vf", &filter_str,
            "-c:v", &encoder.video_codec,
            "-c:a", "copy",
            "-pix_fmt", "yuv420p",
            "-movflags", "+faststart",
            "-threads", "4",
            "-y", &output_str,
        ];
        
        match run_ffmpeg_fast(&args) {
            Ok(_) => {
                info!("字幕方法1 (ass 双引号) 成功");
                succeeded = true;
            }
            Err(e) => {
                info!("字幕方法1 (ass 双引号) 失败: {}", e);
            }
        }
    }
    
    // 方法2: 使用 subtitles 滤镜，双引号包裹路径
    if !succeeded {
        let filter_str = format!("subtitles=\"{}\":si=0", &subtitle_to_use);
        info!("尝试方法2 (subtitles 双引号): {}", filter_str);
        
        let mut args: Vec<&str> = vec![
            "-hide_banner",
            "-loglevel", "error",
            "-i", &input_str,
            "-vf", &filter_str,
            "-c:v", &encoder.video_codec,
            "-c:a", "copy",
            "-pix_fmt", "yuv420p",
            "-movflags", "+faststart",
            "-threads", "4",
            "-y", &output_str,
        ];
        
        match run_ffmpeg_fast(&args) {
            Ok(_) => {
                info!("字幕方法2 (subtitles 双引号) 成功");
                succeeded = true;
            }
            Err(e) => {
                info!("字幕方法2 (subtitles 双引号) 失败: {}", e);
            }
        }
    }
    
    // 方法3: 使用 subtitles 滤镜，原始路径（包含中文和空格）
    if !succeeded {
        let filter_str = format!("subtitles=\"{}\":si=0", subtitle_path);
        info!("尝试方法3 (subtitles 原始路径): {}", filter_str);
        
        let mut args: Vec<&str> = vec![
            "-hide_banner",
            "-loglevel", "error",
            "-i", &input_str,
            "-vf", &filter_str,
            "-c:v", &encoder.video_codec,
            "-c:a", "copy",
            "-pix_fmt", "yuv420p",
            "-movflags", "+faststart",
            "-threads", "4",
            "-y", &output_str,
        ];
        
        match run_ffmpeg_fast(&args) {
            Ok(_) => {
                info!("字幕方法3 (subtitles 原始路径) 成功");
                succeeded = true;
            }
            Err(e) => {
                info!("字幕方法3 (subtitles 原始路径) 失败: {}", e);
            }
        }
    }
    
    // 方法4: 将 SRT 转换为 ASS 格式
    if !succeeded {
        let ass_subtitle_path = temp_dir.join("temp_subtitle.ass");
        
        // 读取 SRT 文件
        let srt_content = match fs::read_to_string(&temp_subtitle_path) {
            Ok(content) => content,
            Err(_) => {
                info!("无法读取 SRT 文件");
                String::new()
            }
        };
        
        if !srt_content.is_empty() {
            // 转换 SRT 到 ASS
            let ass_content = convert_srt_to_ass(&srt_content);
            
            if let Ok(_) = fs::write(&ass_subtitle_path, &ass_content) {
                let filter_str = format!("ass=\"{}\"", ass_subtitle_path.to_string_lossy());
                info!("尝试方法4 (ass 转换格式): {}", filter_str);
                
                let mut args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-i", &input_str,
                    "-vf", &filter_str,
                    "-c:v", &encoder.video_codec,
                    "-c:a", "copy",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &output_str,
                ];
                
                match run_ffmpeg_fast(&args) {
                    Ok(_) => {
                        info!("字幕方法4 (ass 转换格式) 成功");
                        succeeded = true;
                    }
                    Err(e) => {
                        info!("字幕方法4 (ass 转换格式) 失败: {}", e);
                    }
                }
            }
        }
    }
    
    // 清理临时文件
    let _ = std::fs::remove_file(temp_subtitle_path);
    
    if !succeeded {
        info!("所有字幕方法都失败了，跳过字幕，保留原始视频");
        fs::copy(input_path, output_path)
            .map_err(|e| format!("无法复制视频文件: {}", e))?;
    }
    
    Ok(())
}

fn convert_srt_to_ass(srt_content: &str) -> String {
    let mut ass_lines = Vec::new();
    
    // ASS 头部
    ass_lines.push("[Script Info]".to_string());
    ass_lines.push("Title: Converted from SRT".to_string());
    ass_lines.push("ScriptType: v4.00+".to_string());
    ass_lines.push("".to_string());
    
    // 样式
    ass_lines.push("[V4+ Styles]".to_string());
    ass_lines.push("Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding".to_string());
    ass_lines.push("Style: Default,Arial,48,&H00FFFFFF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,2,2,2,10,10,10,1".to_string());
    ass_lines.push("".to_string());
    
    // 事件
    ass_lines.push("[Events]".to_string());
    ass_lines.push("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text".to_string());
    
    // 解析 SRT
    let blocks: Vec<&str> = srt_content.split("\n\n").collect();
    
    for block in blocks {
        let lines: Vec<&str> = block.lines().collect();
        if lines.len() >= 3 {
            // 跳过序号
            let time_line = lines.get(1).unwrap_or(&"");
            let text_lines = &lines[2..];
            
            // 解析时间
            if let Some((start, end)) = parse_srt_time(time_line) {
                let text = text_lines.join("\\N");
                let clean_text = clean_srt_text(&text);
                
                if !clean_text.is_empty() {
                    let ass_start = format_srt_time_to_ass(start);
                    let ass_end = format_srt_time_to_ass(end);
                    ass_lines.push(format!("Dialogue: 0,{},{},Default,,0,0,0,,{}", ass_start, ass_end, clean_text));
                }
            }
        }
    }
    
    ass_lines.join("\n")
}

fn parse_srt_time(time_line: &str) -> Option<(String, String)> {
    // 格式: 00:00:00,000 --> 00:00:00,000
    let parts: Vec<&str> = time_line.split("-->").collect();
    if parts.len() == 2 {
        let start = parts[0].trim().replace(",", ".");
        let end = parts[1].trim().replace(",", ".");
        Some((start, end))
    } else {
        None
    }
}

fn format_srt_time_to_ass(time: String) -> String {
    // 将 00:00:00.000 转换为 0:00:00.00
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() == 3 {
        let hours = parts[0];
        let minutes = parts[1];
        let seconds = parts[2];
        format!("{}:{}:{}", hours, minutes, seconds)
    } else {
        time
    }
}

fn clean_srt_text(text: &str) -> String {
    text.replace("<i>", "")
        .replace("</i>", "")
        .replace("<b>", "")
        .replace("</b>", "")
        .replace("<u>", "")
        .replace("</u>", "")
        .trim()
        .to_string()
}

