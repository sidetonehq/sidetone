//! Flight plan tab: import the latest SimBrief OFP (filing arrives with the network client).

use super::Model;
use crate::app::Action;
use sidetone_ui::imgui::Ui;
use sidetone_ui::theme;

pub fn build(ui: &Ui, m: &mut Model) {
    ui.spacing();
    if !m.simbrief_user_saved {
        ui.text_disabled("Add your SimBrief Pilot ID in Settings to import your latest OFP.");
    }
    let busy = m.simbrief_status.as_deref() == Some("Importing…");
    {
        let _disabled = ui.begin_disabled_with_cond(busy || !m.simbrief_user_saved);
        if ui.button(if m.simbrief.is_some() { "Re-import from SimBrief" } else { "Import from SimBrief" }) {
            m.actions.push(Action::ImportSimBrief);
        }
    }
    if let Some(status) = &m.simbrief_status {
        ui.same_line();
        ui.text_colored(if busy { theme::TEXT_DIM } else { theme::WARN }, status);
    }
    ui.spacing();

    let Some(plan) = &m.simbrief else {
        ui.text_disabled("No plan imported. Filing directly to VATSIM arrives with the network client.");
        return;
    };

    ui.text_colored(theme::ACCENT, &plan.callsign);
    ui.same_line();
    ui.text(format!("{} {}", plan.aircraft_icao, plan.registration));
    ui.separator();
    let name = |icao: &str| m.state.network.vatspy.as_ref().and_then(|v| v.airport_name(icao));
    let airport = |icao: &str, runway: &str| {
        let mut text = icao.to_string();
        if let Some(n) = name(icao) {
            text.push_str(&format!("  {n}"));
        }
        if !runway.is_empty() {
            text.push_str(&format!("  · RWY {runway}"));
        }
        text
    };
    row(ui, "From", &airport(&plan.origin, &plan.origin_runway));
    row(ui, "To", &airport(&plan.destination, &plan.destination_runway));
    row(ui, "Alternate", &if plan.alternate.is_empty() { "—".to_string() } else { airport(&plan.alternate, "") });
    row(ui, "Cruise", &plan.cruise_label());
    row(ui, "Time en route", &plan.ete_label());
    row(ui, "Cost index", &plan.cost_index);
    ui.spacing();
    ui.text_disabled("ROUTE");
    ui.text_wrapped(&plan.route);
    if !plan.icao_flight_plan.is_empty() {
        ui.spacing();
        ui.text_disabled("ICAO FLIGHT PLAN");
        ui.text_wrapped(&plan.icao_flight_plan);
    }
    ui.spacing();
    ui.text_disabled("These airports now drive your ATIS/METAR watch and PDC requests (unless set manually in the ATC tab).");
}

fn row(ui: &Ui, label: &str, value: &str) {
    ui.text_disabled(label);
    ui.same_line_with_pos(120.0 * ui.current_font_size() / theme::FONT_SIZE);
    ui.text(value);
}
