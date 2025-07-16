export type Json =
  | string
  | number
  | boolean
  | null
  | { [key: string]: Json | undefined }
  | Json[]

export type Database = {
  // Allows to automatically instanciate createClient with right options
  // instead of createClient<Database, { PostgrestVersion: 'XX' }>(URL, KEY)
  __InternalSupabase: {
    PostgrestVersion: "12.2.3 (519615d)"
  }
  public: {
    Tables: {
      communication_events: {
        Row: {
          anonymized_content: string | null
          content_hash: string
          event_type: string
          id: string
          metadata: Json | null
          occurred_at: string
          organization_id: string
          processed_at: string
          source_id: string
          unit_id: string | null
        }
        Insert: {
          anonymized_content?: string | null
          content_hash: string
          event_type: string
          id?: string
          metadata?: Json | null
          occurred_at: string
          organization_id: string
          processed_at?: string
          source_id: string
          unit_id?: string | null
        }
        Update: {
          anonymized_content?: string | null
          content_hash?: string
          event_type?: string
          id?: string
          metadata?: Json | null
          occurred_at?: string
          organization_id?: string
          processed_at?: string
          source_id?: string
          unit_id?: string | null
        }
        Relationships: [
          {
            foreignKeyName: "communication_events_organization_id_fkey"
            columns: ["organization_id"]
            isOneToOne: false
            referencedRelation: "organizations"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "communication_events_source_id_fkey"
            columns: ["source_id"]
            isOneToOne: false
            referencedRelation: "data_sources"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "communication_events_unit_id_fkey"
            columns: ["unit_id"]
            isOneToOne: false
            referencedRelation: "organizational_units"
            referencedColumns: ["id"]
          },
        ]
      }
      data_sources: {
        Row: {
          created_at: string
          id: string
          is_active: boolean
          last_sync: string | null
          organization_id: string
          source_config: Json
          source_type: string
        }
        Insert: {
          created_at?: string
          id?: string
          is_active?: boolean
          last_sync?: string | null
          organization_id: string
          source_config: Json
          source_type: string
        }
        Update: {
          created_at?: string
          id?: string
          is_active?: boolean
          last_sync?: string | null
          organization_id?: string
          source_config?: Json
          source_type?: string
        }
        Relationships: [
          {
            foreignKeyName: "data_sources_organization_id_fkey"
            columns: ["organization_id"]
            isOneToOne: false
            referencedRelation: "organizations"
            referencedColumns: ["id"]
          },
        ]
      }
      linguistic_analyses: {
        Row: {
          analysis_type: string
          baseline_metrics: Json | null
          confidence_level: number | null
          created_at: string
          id: string
          metrics: Json
          organization_id: string
          time_window_end: string
          time_window_start: string
          unit_id: string | null
          variance_score: number | null
        }
        Insert: {
          analysis_type: string
          baseline_metrics?: Json | null
          confidence_level?: number | null
          created_at?: string
          id?: string
          metrics: Json
          organization_id: string
          time_window_end: string
          time_window_start: string
          unit_id?: string | null
          variance_score?: number | null
        }
        Update: {
          analysis_type?: string
          baseline_metrics?: Json | null
          confidence_level?: number | null
          created_at?: string
          id?: string
          metrics?: Json
          organization_id?: string
          time_window_end?: string
          time_window_start?: string
          unit_id?: string | null
          variance_score?: number | null
        }
        Relationships: [
          {
            foreignKeyName: "linguistic_analyses_organization_id_fkey"
            columns: ["organization_id"]
            isOneToOne: false
            referencedRelation: "organizations"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "linguistic_analyses_unit_id_fkey"
            columns: ["unit_id"]
            isOneToOne: false
            referencedRelation: "organizational_units"
            referencedColumns: ["id"]
          },
        ]
      }
      mission_resonance: {
        Row: {
          drift_indicators: Json | null
          id: string
          linguistic_distance: number | null
          measured_at: string
          organization_id: string
          resonance_score: number
          unit_id: string | null
        }
        Insert: {
          drift_indicators?: Json | null
          id?: string
          linguistic_distance?: number | null
          measured_at?: string
          organization_id: string
          resonance_score: number
          unit_id?: string | null
        }
        Update: {
          drift_indicators?: Json | null
          id?: string
          linguistic_distance?: number | null
          measured_at?: string
          organization_id?: string
          resonance_score?: number
          unit_id?: string | null
        }
        Relationships: [
          {
            foreignKeyName: "mission_resonance_organization_id_fkey"
            columns: ["organization_id"]
            isOneToOne: false
            referencedRelation: "organizations"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "mission_resonance_unit_id_fkey"
            columns: ["unit_id"]
            isOneToOne: false
            referencedRelation: "organizational_units"
            referencedColumns: ["id"]
          },
        ]
      }
      organizational_units: {
        Row: {
          created_at: string
          id: string
          name: string
          organization_id: string
          parent_unit_id: string | null
          unit_type: string
        }
        Insert: {
          created_at?: string
          id?: string
          name: string
          organization_id: string
          parent_unit_id?: string | null
          unit_type: string
        }
        Update: {
          created_at?: string
          id?: string
          name?: string
          organization_id?: string
          parent_unit_id?: string | null
          unit_type?: string
        }
        Relationships: [
          {
            foreignKeyName: "organizational_units_organization_id_fkey"
            columns: ["organization_id"]
            isOneToOne: false
            referencedRelation: "organizations"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "organizational_units_parent_unit_id_fkey"
            columns: ["parent_unit_id"]
            isOneToOne: false
            referencedRelation: "organizational_units"
            referencedColumns: ["id"]
          },
        ]
      }
      organizations: {
        Row: {
          created_at: string
          id: string
          mission_statement: string | null
          name: string
          updated_at: string
          vision_statement: string | null
        }
        Insert: {
          created_at?: string
          id?: string
          mission_statement?: string | null
          name: string
          updated_at?: string
          vision_statement?: string | null
        }
        Update: {
          created_at?: string
          id?: string
          mission_statement?: string | null
          name?: string
          updated_at?: string
          vision_statement?: string | null
        }
        Relationships: []
      }
      stabilization_interventions: {
        Row: {
          alert_id: string
          content: string
          created_at: string
          effectiveness_score: number | null
          id: string
          implementation_status: string
          implemented_at: string | null
          intervention_type: string
        }
        Insert: {
          alert_id: string
          content: string
          created_at?: string
          effectiveness_score?: number | null
          id?: string
          implementation_status?: string
          implemented_at?: string | null
          intervention_type: string
        }
        Update: {
          alert_id?: string
          content?: string
          created_at?: string
          effectiveness_score?: number | null
          id?: string
          implementation_status?: string
          implemented_at?: string | null
          intervention_type?: string
        }
        Relationships: [
          {
            foreignKeyName: "stabilization_interventions_alert_id_fkey"
            columns: ["alert_id"]
            isOneToOne: false
            referencedRelation: "symbolic_alerts"
            referencedColumns: ["id"]
          },
        ]
      }
      symbolic_alerts: {
        Row: {
          alert_type: string
          created_at: string
          description: string
          id: string
          interpretive_analysis: string | null
          is_resolved: boolean
          organization_id: string
          recommended_actions: Json | null
          resolved_at: string | null
          severity: string
          title: string
          unit_id: string | null
        }
        Insert: {
          alert_type: string
          created_at?: string
          description: string
          id?: string
          interpretive_analysis?: string | null
          is_resolved?: boolean
          organization_id: string
          recommended_actions?: Json | null
          resolved_at?: string | null
          severity: string
          title: string
          unit_id?: string | null
        }
        Update: {
          alert_type?: string
          created_at?: string
          description?: string
          id?: string
          interpretive_analysis?: string | null
          is_resolved?: boolean
          organization_id?: string
          recommended_actions?: Json | null
          resolved_at?: string | null
          severity?: string
          title?: string
          unit_id?: string | null
        }
        Relationships: [
          {
            foreignKeyName: "symbolic_alerts_organization_id_fkey"
            columns: ["organization_id"]
            isOneToOne: false
            referencedRelation: "organizations"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "symbolic_alerts_unit_id_fkey"
            columns: ["unit_id"]
            isOneToOne: false
            referencedRelation: "organizational_units"
            referencedColumns: ["id"]
          },
        ]
      }
      symbolic_baselines: {
        Row: {
          baseline_data: Json
          baseline_type: string
          established_at: string
          id: string
          last_updated: string
          organization_id: string
          time_period_days: number
          unit_id: string | null
        }
        Insert: {
          baseline_data: Json
          baseline_type: string
          established_at?: string
          id?: string
          last_updated?: string
          organization_id: string
          time_period_days: number
          unit_id?: string | null
        }
        Update: {
          baseline_data?: Json
          baseline_type?: string
          established_at?: string
          id?: string
          last_updated?: string
          organization_id?: string
          time_period_days?: number
          unit_id?: string | null
        }
        Relationships: [
          {
            foreignKeyName: "symbolic_baselines_organization_id_fkey"
            columns: ["organization_id"]
            isOneToOne: false
            referencedRelation: "organizations"
            referencedColumns: ["id"]
          },
          {
            foreignKeyName: "symbolic_baselines_unit_id_fkey"
            columns: ["unit_id"]
            isOneToOne: false
            referencedRelation: "organizational_units"
            referencedColumns: ["id"]
          },
        ]
      }
    }
    Views: {
      [_ in never]: never
    }
    Functions: {
      [_ in never]: never
    }
    Enums: {
      [_ in never]: never
    }
    CompositeTypes: {
      [_ in never]: never
    }
  }
}

type DatabaseWithoutInternals = Omit<Database, "__InternalSupabase">

type DefaultSchema = DatabaseWithoutInternals[Extract<keyof Database, "public">]

export type Tables<
  DefaultSchemaTableNameOrOptions extends
    | keyof (DefaultSchema["Tables"] & DefaultSchema["Views"])
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof (DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"] &
        DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Views"])
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? (DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"] &
      DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Views"])[TableName] extends {
      Row: infer R
    }
    ? R
    : never
  : DefaultSchemaTableNameOrOptions extends keyof (DefaultSchema["Tables"] &
        DefaultSchema["Views"])
    ? (DefaultSchema["Tables"] &
        DefaultSchema["Views"])[DefaultSchemaTableNameOrOptions] extends {
        Row: infer R
      }
      ? R
      : never
    : never

export type TablesInsert<
  DefaultSchemaTableNameOrOptions extends
    | keyof DefaultSchema["Tables"]
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"]
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"][TableName] extends {
      Insert: infer I
    }
    ? I
    : never
  : DefaultSchemaTableNameOrOptions extends keyof DefaultSchema["Tables"]
    ? DefaultSchema["Tables"][DefaultSchemaTableNameOrOptions] extends {
        Insert: infer I
      }
      ? I
      : never
    : never

export type TablesUpdate<
  DefaultSchemaTableNameOrOptions extends
    | keyof DefaultSchema["Tables"]
    | { schema: keyof DatabaseWithoutInternals },
  TableName extends DefaultSchemaTableNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"]
    : never = never,
> = DefaultSchemaTableNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaTableNameOrOptions["schema"]]["Tables"][TableName] extends {
      Update: infer U
    }
    ? U
    : never
  : DefaultSchemaTableNameOrOptions extends keyof DefaultSchema["Tables"]
    ? DefaultSchema["Tables"][DefaultSchemaTableNameOrOptions] extends {
        Update: infer U
      }
      ? U
      : never
    : never

export type Enums<
  DefaultSchemaEnumNameOrOptions extends
    | keyof DefaultSchema["Enums"]
    | { schema: keyof DatabaseWithoutInternals },
  EnumName extends DefaultSchemaEnumNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[DefaultSchemaEnumNameOrOptions["schema"]]["Enums"]
    : never = never,
> = DefaultSchemaEnumNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[DefaultSchemaEnumNameOrOptions["schema"]]["Enums"][EnumName]
  : DefaultSchemaEnumNameOrOptions extends keyof DefaultSchema["Enums"]
    ? DefaultSchema["Enums"][DefaultSchemaEnumNameOrOptions]
    : never

export type CompositeTypes<
  PublicCompositeTypeNameOrOptions extends
    | keyof DefaultSchema["CompositeTypes"]
    | { schema: keyof DatabaseWithoutInternals },
  CompositeTypeName extends PublicCompositeTypeNameOrOptions extends {
    schema: keyof DatabaseWithoutInternals
  }
    ? keyof DatabaseWithoutInternals[PublicCompositeTypeNameOrOptions["schema"]]["CompositeTypes"]
    : never = never,
> = PublicCompositeTypeNameOrOptions extends {
  schema: keyof DatabaseWithoutInternals
}
  ? DatabaseWithoutInternals[PublicCompositeTypeNameOrOptions["schema"]]["CompositeTypes"][CompositeTypeName]
  : PublicCompositeTypeNameOrOptions extends keyof DefaultSchema["CompositeTypes"]
    ? DefaultSchema["CompositeTypes"][PublicCompositeTypeNameOrOptions]
    : never

export const Constants = {
  public: {
    Enums: {},
  },
} as const
