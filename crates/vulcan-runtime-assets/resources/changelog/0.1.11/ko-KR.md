Vulcan Code 0.1.11은 피드백, 메시지 센터, 내 기기에서 문제를 고치는 방법을 추가하고 OpenAI, TokenRhythm, OpenCode Go, OpenDesign의 모델을 업데이트합니다.

### 이 버전으로 업데이트

```ui-checklist
ok | 증분 업데이트 | 0.1.8, 0.1.9 또는 0.1.10에서 업데이트하면 변경된 파일만 내려받습니다. 그 이전 버전은 계속 전체 업데이트로 설치됩니다
ok | 앱 데이터베이스 | 처음 시작할 때 앱 데이터베이스를 백업한 뒤 새 구조로 자동 업그레이드합니다. 따로 할 일은 없습니다
```

### 피드백과 메시지

```ui-checklist
ok | 피드백 보내기 | 로그인하면 "피드백 보내기"로 문제나 제안을 보낼 수 있습니다. 보내기 전에 비밀 값, 경로, 사용자 이름, URL, 개인 정보를 기기에서 걸러 내고 실제로 보낼 내용을 그대로 미리 봅니다
ok | 오류 카드에서 보고 | 대화가 실패하면 오류 카드에서 한 번에 보고할 수 있습니다. Vulcan Code 때문일 수 있는 실패에만 표시되며 인증, 잔액, 속도 제한, 서비스 장애, 시간 초과, 콘텐츠 정책, 취소에는 표시되지 않습니다
ok | 메시지 센터 | "메시지 센터"에서 공지, 개인 메시지, 피드백에 대한 관리자 답장을 확인합니다. 중요한 공지는 배너로도 표시됩니다
ok | 이 기기에서 수정하기 | 폴더를 고르면 vulcan-code 체크아웃이 프로젝트로 준비되고 수정 스킬이 있는 대화가 열립니다. 그 대화에서 풀 리퀘스트를 확인하기 전에는 아무것도 푸시하거나 게시하지 않습니다
ok | GitHub 항목 | 계정 메뉴에 "GitHub에 제안하기 (Issue)"와 "GitHub에 PR 보내기"가 추가되었습니다
ok | 터미널 클라이언트 | 터미널 클라이언트에도 피드백 페이지가 있습니다
```

### 모델과 공급자

```ui-checklist
ok | OpenAI | 공개 API의 8개 모델이 엄격한 json_schema 구조화 출력을 선언합니다. GPT-5.6 Sol/Terra/Luna, GPT-6 Sol/Luna, GPT-6.1 Terra는 추론을 끌 수 있습니다
ok | TokenRhythm | DeepSeek V4.1 Flash 1017, LongCat 2.0, MiniMax M3.1 Flash와 테스트 모델 Egg-Alpha, Neohorse 1 9B를 추가하고 minimax-m2.5와 mimo-v2.5-pro를 제거했습니다. 구조화 출력을 선언하고, Messages 모델은 사고 강도와 사고 끄기를 보내며, DeepSeek V4 Pro 0813과 GLM-5.2는 Responses로, Kimi K2.6은 Chat으로 옮겼습니다
ok | OpenCode Go | Gemini 4 Argon, Hy4와 무료 모델 Beam, Iceman, Crouton을 추가했습니다. 10개 모델이 구조화 출력을 선언합니다
ok | OpenDesign | GPT-6 Luna는 Responses, Claude Haiku 5.5는 Messages를 사용합니다. deepseek-v4.1-flash-eco, hy4-preview, minimax-m3.1-flash와 14개 Plus/Pro/Max 채팅 모델을 추가했습니다. 요금제에 포함된 경우 GPT Image 2, Nano Banana 2/2 Lite, Seedream 5.0/5.0 Pro로 이미지를 생성할 수 있습니다. 사고 수준은 각 공급사가 공개한 것만 표시합니다
ok | 메모리 모델 | 사고를 끄고 구조화 출력을 반환할 수 있는 모델이 늘어 더 많은 모델을 메모리 모델로 선택할 수 있습니다
```
