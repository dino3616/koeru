//! KOERU の音声入出力。
//!
//! **各 OS の API を直接叩き、抽象レイヤを挟まない**（`DEC-REC-001`）。
//! `TR-REC-08`〜`12` が要求する制御（`IAudioEffectsManager`、
//! `IAudioClient2::SetClientProperties`、`AVCaptureDevice.activeMicrophoneMode`、
//! PipeWire のノード選択）はどれもバックエンド固有で、抽象が出すものではない。
//!
//! このクレートは2層に分かれる。
//!
//! - [`session`] — 収録セッションの状態機械。**純粋で、I/O を持たない。**
//!   `specs/requirements/recording-input.fsl`（`proved`）の写し
//! - バックエンド — OS の API を叩き、出来事を [`session::Session`] へ渡す（未実装）
//!
//! **状態機械を純粋にしてあるのは、ハードウェアなしで契約を検査できるようにするため。**

pub mod device;
pub mod error;
pub mod session;

pub use device::{DeviceId, DeviceInfo, RedactedName};
pub use error::SessionError;
pub use session::{Device, Effects, Gain, Liveness, Session};

#[cfg(test)]
mod contract_tests {
    //! `recording-input.fsl` の acceptance / forbidden を、そのまま写したテスト。
    //!
    //! **FSL 側で検証済みの経路が、Rust でも同じ結果になることを確かめる。**
    //! シナリオを足すときは FSL を先に直す。

    use super::*;

    fn dev() -> DeviceId {
        DeviceId::new("test-endpoint-0")
    }

    /// 収録開始まで到達させる共通の手順。
    fn ready() -> Session {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        s.effects_all_disabled().expect("効果を無効化できる");
        s.calibrate_gain().expect("校正できる");
        s.input_is_alive().expect("入力が届いている");
        s.estimate_space(100, 1000).expect("残量を見積もれる");
        s
    }

    /// AC-REC-101 選んで、開いて、効果を無効化し、校正し、生死と残量を確かめて録る
    #[test]
    fn ac_rec_101_一連の手順でテイクが1つ確定しストリームは開いたまま() {
        let mut s = ready();
        s.start_take().expect("収録を始められる");
        s.finish_take().expect("テイクを確定できる");
        assert_eq!(s.takes(), 1);
        assert!(s.is_stream_open(), "テイクが確定してもストリームは閉じない");
    }

    /// FB-REC-101 デバイスが消失したまま収録を始められない
    #[test]
    fn fb_rec_101_デバイス消失後は収録を始められない() {
        let mut s = ready();
        s.device_lost().expect("消失を記録できる");
        assert!(s.start_take().is_err());
    }

    /// FB-REC-102 入力が届いていないまま収録を始められない
    #[test]
    fn fb_rec_102_入力が死んでいると収録を始められない() {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        s.effects_all_disabled().expect("効果を無効化できる");
        s.calibrate_gain().expect("校正できる");
        s.input_is_dead().expect("死を記録できる");
        s.estimate_space(100, 1000).expect("残量を見積もれる");
        assert!(s.start_take().is_err());
        assert_eq!(s.device(), Device::NotSelected, "デバイス選択へ戻る");
    }

    /// FB-REC-103 校正しないまま収録を始められない
    #[test]
    fn fb_rec_103_校正していないと収録を始められない() {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        s.effects_all_disabled().expect("効果を無効化できる");
        s.input_is_alive().expect("入力が届いている");
        s.estimate_space(100, 1000).expect("残量を見積もれる");
        assert!(matches!(
            s.start_take(),
            Err(SessionError::GainState { .. })
        ));
    }

    /// FB-REC-104 収録中に手順の提示を出せない
    #[test]
    fn fb_rec_104_収録中は手順を提示できない() {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        s.effects_some_remain().expect("効果が残ることを記録できる");
        s.calibrate_gain().expect("校正できる");
        s.input_is_alive().expect("入力が届いている");
        s.estimate_space(100, 1000).expect("残量を見積もれる");
        s.start_take().expect("収録を始められる");
        assert!(matches!(
            s.show_prompt_once(),
            Err(SessionError::RecordingState { .. })
        ));
    }

    /// FB-REC-105 回り込みを確認しないままガイドを鳴らせない
    #[test]
    fn fb_rec_105_回り込み未確認ではガイドを鳴らせない() {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        assert!(matches!(
            s.enable_guide(),
            Err(SessionError::LeakNotChecked)
        ));
    }

    /// FB-REC-106 残量が足りないまま収録を始められない
    #[test]
    fn fb_rec_106_残量不足では収録を始められない() {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        s.effects_all_disabled().expect("効果を無効化できる");
        s.calibrate_gain().expect("校正できる");
        s.input_is_alive().expect("入力が届いている");
        s.estimate_space(1000, 100).expect("見積もり自体は成功する");
        assert!(matches!(s.start_take(), Err(SessionError::NotEnoughSpace)));
    }

    /// FB-REC-107 残量を見積もらないまま収録を始められない
    #[test]
    fn fb_rec_107_未見積もりでは収録を始められない() {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        s.effects_all_disabled().expect("効果を無効化できる");
        s.calibrate_gain().expect("校正できる");
        s.input_is_alive().expect("入力が届いている");
        assert!(matches!(s.start_take(), Err(SessionError::NotEnoughSpace)));
    }

    /// INV-REC-108 手順の提示は多くとも一度しか出ない
    #[test]
    fn inv_rec_108_手順は二度提示できない() {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        s.effects_some_remain().expect("効果が残ることを記録できる");
        s.show_prompt_once().expect("一度は提示できる");
        assert!(matches!(
            s.show_prompt_once(),
            Err(SessionError::PromptAlreadyShown)
        ));
        assert_eq!(s.prompts_shown(), 1);
    }

    /// REQ-REC-109 復帰は同一識別子のデバイスが戻ったときだけ
    #[test]
    fn req_rec_109_別のデバイスでは復帰しない() {
        let mut s = ready();
        s.device_lost().expect("消失を記録できる");
        assert!(
            s.same_device_returned(&DeviceId::new("別のデバイス"))
                .is_err(),
            "別のデバイスへ自動で切り替えない"
        );
        assert_eq!(s.device(), Device::Lost);
        s.same_device_returned(&dev())
            .expect("同一識別子なら戻れる");
        assert_eq!(s.device(), Device::Selected);
    }

    /// ResumesAfterDeviceReturn デバイスが戻れば収録を再開できる
    #[test]
    fn 到達性_デバイス復帰後に収録を再開できる() {
        let mut s = ready();
        s.start_take().expect("収録を始められる");
        s.finish_take().expect("テイクを確定できる");
        s.device_lost().expect("消失を記録できる");
        s.same_device_returned(&dev())
            .expect("同一識別子なら戻れる");
        // 消失でストリームと生死判定が落ちるので、開き直して確かめ直す
        s.open_stream().expect("開き直せる");
        s.input_is_alive().expect("入力が届いている");
        s.start_take().expect("再開できる");
        assert!(s.is_recording());
    }

    /// RecordsWithRemainingEffects 無効化できない効果が残ったまま、それでも収録へ進める
    #[test]
    fn 到達性_効果が残ったままでも収録へ進める() {
        let mut s = Session::new(3);
        s.select_device(dev()).expect("デバイスを選べる");
        s.open_stream().expect("ストリームを開ける");
        s.effects_some_remain().expect("効果が残ることを記録できる");
        s.show_prompt_once().expect("一度だけ提示できる");
        s.calibrate_gain().expect("校正できる");
        s.input_is_alive().expect("入力が届いている");
        s.estimate_space(100, 1000).expect("残量を見積もれる");
        s.start_take().expect("収録を始められる");
        s.finish_take().expect("テイクを確定できる");
        assert_eq!(s.takes(), 1);
        assert_eq!(s.effects(), Effects::SomeRemain);
        assert_eq!(s.prompts_shown(), 1);
    }

    /// INV-REC-107 アプリが OS 側のゲインを変えたなら、終了時に必ず戻している
    #[test]
    fn inv_rec_107_変えたゲインは終了時に戻す() {
        let mut s = ready();
        s.exit().expect("終了できる");
        assert!(s.os_gain_restored());
    }

    /// REQ-REC-105 収録中にゲインを変えない
    #[test]
    fn req_rec_105_収録中は校正できない() {
        let mut s = ready();
        s.start_take().expect("収録を始められる");
        // 収録中の校正は前提で弾かれる
        assert!(matches!(
            s.calibrate_gain(),
            Err(SessionError::GainState { .. } | SessionError::RecordingState { .. })
        ));
    }

    /// 終了後は何もできない
    #[test]
    fn 終了後の操作は拒まれる() {
        let mut s = ready();
        s.exit().expect("終了できる");
        assert!(matches!(s.start_take(), Err(SessionError::Exited)));
        assert!(matches!(s.open_stream(), Err(SessionError::Exited)));
    }

    /// 送信層へ載せる語彙は固定文字列で、Display を含まない
    #[test]
    fn 失敗の種別は固定文字列になる() {
        let e = SessionError::DeviceState {
            expected: Device::Selected,
            actual: Device::Lost,
        };
        assert_eq!(e.kind(), "recording.device_state");
        assert!(!e.kind().contains("Lost"), "kind に状態の中身を混ぜない");
    }
}
