use super::*;
use num_traits::{AsPrimitive, Num};

struct Parser<'tokens, 'text> {
    tokens: &'tokens [&'text [u8]],
    position: usize,
}

/// Parse a definition comment
pub(crate) fn parse_definition(tokens: &[&[u8]]) -> Result<Option<Definition>, String> {
    let mut parser = Parser {
        tokens,
        position: 0,
    };

    parser.run()
}

impl<'text> Parser<'_, 'text> {
    fn get_token(&mut self, context: &str) -> Result<&'text [u8], String> {
        if self.position < self.tokens.len() {
            let token = self.tokens[self.position];
            self.position += 1;
            Ok(token)
        } else {
            Err(format!("Unexpected end of input in {}", context))
        }
    }

    fn require_token(&mut self, context: &str, target: &'static [u8]) -> Result<(), String> {
        // Ensure that the tokens contain the expected target
        let t = self.get_token(context)?;
        if t != target {
            return Err(format!(
                "Expected {} in {}, got {}",
                String::from_utf8_lossy(target),
                context,
                String::from_utf8_lossy(t)
            ));
        }
        Ok(())
    }

    /// Get the next token without consuming it
    fn peek_token(&mut self) -> Option<&'text [u8]> {
        if self.position < self.tokens.len() {
            Some(self.tokens[self.position])
        } else {
            None
        }
    }

    /// Check if the next token matches the expected token
    fn check_next_token(&mut self, expected: &'static [u8]) -> bool {
        if let Some(token) = self.peek_token() {
            token == expected
        } else {
            false
        }
    }

    fn get_identifier(&mut self, context: &str) -> Result<String, String> {
        // the token can't be a quoted string or start with a digit
        if let Some(t) = self.tokens.get(self.position)
            && !t.starts_with(b"\"")
            && !t[0].is_ascii_digit()
        {
            self.position += 1;
            Ok(String::from_utf8_lossy(t).into_owned())
        } else {
            Err(format!("Expected identifier in {} definition", context))
        }
    }

    fn get_string(&mut self, context: &str) -> Result<String, String> {
        if let Some(t) = self.tokens.get(self.position)
            && t.starts_with(b"\"")
            && t.ends_with(b"\"")
        {
            // only increment the position if we found a valid string
            self.position += 1;
            // return the string without the surrounding quotes
            Ok(String::from_utf8_lossy(&t[1..t.len() - 1]).into_owned())
        } else {
            // no token or not quoted -> reject
            Err(format!("Expected quoted string in {} definition", context))
        }
    }

    fn get_uint_value<T>(&mut self, context: &str) -> Result<T, String>
    where
        T: Num + std::str::FromStr + Copy + 'static,
        u64: AsPrimitive<T>,
    {
        if let Some(token) = self.tokens.get(self.position) {
            let std::borrow::Cow::Borrowed(value_string) = String::from_utf8_lossy(token) else {
                return Err(format!("Expected value in {context}, found invalid UTF-8",));
            };

            if value_string.starts_with("-0x") {
                // -0x... negative hex number
                return Err(format!(
                    "Expected unsigned value in {context}, found negative hex: {value_string}"
                ));
            } else if let Some(hexchars) = value_string.strip_prefix("0x") {
                // 0x... positive(?) hex number - if the sign bit is set, these numbers are interpreted as negative too!
                if let Ok(value) = u64::from_str_radix(hexchars, 16) {
                    self.position += 1;
                    return Ok(value.as_());
                }
            } else {
                // decimal
                if let Ok(value) = value_string.parse() {
                    self.position += 1;
                    return Ok(value);
                }
            }

            Err(format!("Expected value in {context}, got {value_string}"))
        } else {
            Err(format!("Expected integer value in {context}"))
        }
    }

    fn get_float_value(&mut self, context: &str) -> Result<f64, String> {
        if let Some(token) = self.tokens.get(self.position) {
            let value = convert_float_value(token, context)?;
            self.position += 1;
            Ok(value)
        } else {
            Err(format!("Expected float value in {context}"))
        }
    }

    fn run(&mut self) -> Result<Option<Definition>, String> {
        let cmd_type_token = self.get_token("Definition")?;
        match cmd_type_token {
            b"SYMBOL" => Ok(Some(self.parse_symbol()?)),
            b"SUB_GROUP" => Ok(Some(Definition::SubGroup(self.parse_sub_group()?))),
            b"MAIN_GROUP" => Ok(Some(Definition::MainGroup(self.parse_main_group()?))),
            b"CONVERSION" => Ok(Some(self.parse_conversion()?)),
            b"ELEMENT" => Ok(Some(self.parse_element()?)),
            b"INSTANCE" => Ok(Some(Definition::Instance(self.parse_instance()?))),
            b"SUB_STRUCTURE" => Ok(Some(self.parse_sub_structure()?)),
            b"VAR_CRITERION" => Ok(Some(Definition::VarCriterion(self.parse_var_criterion()?))),
            _ => {
                // Unknown definition type - don't return an error here, since the @@
                // in the comment might be part of other markup, like doc comments
                Ok(None)
            }
        }
    }

    fn parse_symbol(&mut self) -> Result<Definition, String> {
        // Parse the SYMBOL definition
        self.require_token("SYMBOL", b"=")?;
        let name = self.get_identifier("SYMBOL")?;
        self.require_token("SYMBOL", b"A2L_TYPE")?;
        self.require_token("SYMBOL", b"=")?;

        let a2l_type_tok = self.get_token("SYMBOL")?;

        let (config, a2l_name) = match a2l_type_tok {
            b"MEASURE" => self.parse_measure_config()?,
            b"PARAMETER" => self.parse_parameter_config()?,
            b"CURVE" => self.parse_curve_map_config(false)?,
            b"MAP" => self.parse_curve_map_config(true)?,
            b"AXIS" => self.parse_axis_config()?,
            b"STRING" => self.parse_string_config()?,
            other => {
                return Err(format!(
                    "Expected A2L_TYPE after SYMBOL keyword, found: {:?}",
                    String::from_utf8_lossy(other)
                ));
            }
        };
        let a2l_name = a2l_name.unwrap_or(name.clone());
        Ok(Definition::Symbol(SymbolDefinition {
            symbol_name: name,
            a2l_name,
            config,
        }))
    }

    fn parse_measure_config(&mut self) -> Result<(ItemConfig, Option<String>), String> {
        // Parse the MEASURE config
        let write_access = self.parse_write_access();
        let a2l_name = self.parse_a2l_name(&[b"DATA_TYPE"]);
        let (datatype, bitmask) = self.parse_data_type()?;
        let range = self.parse_opt_range()?;
        let _extended_range = self.parse_opt_range()?;
        let attributes = self.parse_attributes()?;
        self.require_token("SYMBOL", b"END")?;

        let measure_cfg = MeasureCfg {
            write_access,
            datatype,
            bitmask,
            range,
            attributes,
        };
        Ok((ItemConfig::Measure(measure_cfg), a2l_name))
    }

    fn parse_parameter_config(&mut self) -> Result<(ItemConfig, Option<String>), String> {
        // Parse the PARAMETER config
        let write_access = self.parse_write_access();
        let a2l_name = self.parse_a2l_name(&[b"DATA_TYPE"]);
        let (datatype, bitmask) = self.parse_data_type()?;
        let range = self.parse_opt_range()?;
        let extended_range = self.parse_opt_range()?;
        let attributes = self.parse_attributes()?;
        self.require_token("SYMBOL", b"END")?;

        let parameter_cfg = ParameterCfg {
            write_access,
            datatype,
            bitmask,
            range,
            extended_range,
            attributes,
        };
        Ok((ItemConfig::Parameter(parameter_cfg), a2l_name))
    }

    fn parse_curve_map_config(
        &mut self,
        is_map: bool,
    ) -> Result<(ItemConfig, Option<String>), String> {
        // Parse the CURVE or MAP config
        let context = if is_map { "MAP" } else { "CURVE" };
        let write_access = self.parse_write_access();
        let a2l_name = self.parse_a2l_name(&[b"DATA_TYPE"]);
        let (datatype, bitmask) = self.parse_data_type()?;
        let range = self.parse_opt_range()?;
        let extended_range = self.parse_opt_range()?;
        let layout = self.parse_layout()?;
        let attributes = self.parse_map_attributes()?;
        self.require_token(context, b"X_AXIS")?;
        let x_axis = self.parse_axis()?;
        let y_axis = if is_map {
            self.require_token("MAP", b"Y_AXIS")?;
            Some(self.parse_axis()?)
        } else {
            None
        };
        self.require_token("SYMBOL", b"END")?;

        let curve_map_cfg = CurveMapCfg {
            write_access,
            datatype,
            bitmask,
            range,
            extended_range,
            layout,
            attributes,
            x_axis: Box::new(x_axis),
            y_axis: y_axis.map(Box::new),
        };
        Ok((ItemConfig::CurveMap(curve_map_cfg), a2l_name))
    }

    fn parse_axis_config(&mut self) -> Result<(ItemConfig, Option<String>), String> {
        // Parse the AXIS config
        let write_access = self.parse_write_access();
        let a2l_name = self.parse_a2l_name(&[b"DATA_TYPE"]);
        let (datatype, _bitmask) = self.parse_data_type()?;
        let (range, extended_range) = (self.parse_opt_range()?, self.parse_opt_range()?);
        let layout = self.parse_layout()?;
        self.require_token("AXIS", b"DIMENSION")?;
        let dimension = self.parse_dimension_attribute()?;
        let (input_signal, input_is_instance) = self.parse_axis_input_signal()?;
        let attributes = self.parse_map_attributes()?;
        self.require_token("SYMBOL", b"END")?;

        let axis_cfg = AxisCfg {
            write_access,
            datatype,
            range,
            extended_range,
            layout,
            dimension,
            input_signal,
            input_is_instance,
            attributes,
        };
        Ok((ItemConfig::Axis(axis_cfg), a2l_name))
    }

    fn parse_string_config(&mut self) -> Result<(ItemConfig, Option<String>), String> {
        // Parse the STRING config
        let length = self.get_uint_value("STRING length")?;
        let write_access = self.parse_write_access();
        let a2l_name = self.parse_a2l_name(&[
            b"ADDRESS",
            b"ADDRESS",
            b"ADDRESS_EXTENSION",
            b"ALIAS",
            b"BASE_OFFSET",
            b"DESCRIPTION",
            b"DIMENSION",
            b"END",
            b"GROUP",
            b"VAR_CRITERION",
        ]);
        let attributes = self.parse_string_attributes()?;
        self.require_token("SYMBOL", b"END")?;

        let string_cfg = StringCfg {
            length,
            write_access,
            attributes,
        };
        Ok((ItemConfig::String(string_cfg), a2l_name))
    }

    fn parse_sub_group(&mut self) -> Result<SubGroupDefinition, String> {
        // Parse the SUB_GROUP definition
        self.require_token("SUB_GROUP", b"=")?;
        let name = self.get_identifier("SUB_GROUP")?;
        let description = self.parse_description("SUB_GROUP")?;
        self.require_token("SUB_GROUP", b"END")?;
        Ok(SubGroupDefinition { name, description })
    }

    fn parse_main_group(&mut self) -> Result<MainGroupDefinition, String> {
        // Parse the MAIN_GROUP definition
        self.require_token("MAIN_GROUP", b"=")?;
        let name = self.get_identifier("MAIN_GROUP")?;
        let description = self.parse_description("MAIN_GROUP")?;
        self.require_token("MAIN_GROUP", b"END")?;
        Ok(MainGroupDefinition { name, description })
    }

    fn parse_conversion(&mut self) -> Result<Definition, String> {
        // Parse the CONVERSION definition
        self.require_token("CONVERSION", b"=")?;
        let name = self.get_identifier("CONVERSION")?;
        self.require_token("CONVERSION", b"A2L_TYPE")?;
        self.require_token("CONVERSION", b"=")?;

        let a2l_type_tok = self.get_token("CONVERSION")?;

        match a2l_type_tok {
            b"LINEAR" => self.parse_linear_conversion(name),
            b"FORMULA" => self.parse_formula_conversion(name),
            b"TABLE" => self.parse_table_conversion(name),
            other => Err(format!(
                "Expected A2L_TYPE after CONVERSION keyword, found: {:?}",
                String::from_utf8_lossy(other)
            )),
        }
    }

    fn parse_linear_conversion(&mut self, name: String) -> Result<Definition, String> {
        let factor = self.get_float_value("linear conversion factor")?;
        let offset = self.get_float_value("linear conversion offset")?;
        let unit = self.parse_unit("linear conversion unit")?;
        let description = self.parse_description("linear conversion description")?;

        self.require_token("LINEAR", b"END")?;

        let linear_cfg = LinearCfg { factor, offset };
        Ok(Definition::Conversion(ConversionDefinition {
            name,
            unit,
            description,
            config: ConversionConfig::Linear(linear_cfg),
        }))
    }

    fn parse_formula_conversion(&mut self, name: String) -> Result<Definition, String> {
        let formula = self.get_string("FORMULA")?;

        let inverse_formula = if self.check_next_token(b"INVERSE") {
            let _ = self.get_token("FORMULA"); // consume the INVERSE token
            let inverse_formula = self.get_string("FORMULA conversion inverse formula")?;
            Some(inverse_formula)
        } else {
            None
        };

        let unit = self.parse_unit("formula conversion unit")?;
        let description = self.parse_description("formula conversion description")?;

        self.require_token("FORMULA", b"END")?;

        let formula_cfg = FormulaCfg {
            formula,
            inverse_formula,
        };
        Ok(Definition::Conversion(ConversionDefinition {
            name,
            unit,
            description,
            config: ConversionConfig::Formula(formula_cfg),
        }))
    }

    fn parse_table_conversion(&mut self, name: String) -> Result<Definition, String> {
        let (rows, default_value) = self.parse_conversion_table()?;

        let unit = self.parse_unit("table conversion")?;
        let description = self.parse_description("table conversion description")?;

        self.require_token("TABLE", b"END")?;

        let table_cfg = TableCfg {
            rows,
            default_value,
        };
        Ok(Definition::Conversion(ConversionDefinition {
            name,
            unit,
            description,
            config: ConversionConfig::Table(table_cfg),
        }))
    }

    fn parse_element(&mut self) -> Result<Definition, String> {
        // Parse the ELEMENT definition
        self.require_token("ELEMENT", b"=")?;
        let name = self.get_identifier("ELEMENT")?;
        self.require_token("SYMBOL", b"STRUCTURE")?;
        let structure = self.parse_identifier_list("ELEMENT structure")?;
        self.require_token("SYMBOL", b"A2L_TYPE")?;
        self.require_token("SYMBOL", b"=")?;
        let a2l_type_tok = self.get_token("SYMBOL")?;

        let (config, a2l_name) = match a2l_type_tok {
            b"MEASURE" => self.parse_measure_config()?,
            b"PARAMETER" => self.parse_parameter_config()?,
            b"CURVE" => self.parse_curve_map_config(false)?,
            b"MAP" => self.parse_curve_map_config(true)?,
            b"AXIS" => self.parse_axis_config()?,
            b"STRING" => self.parse_string_config()?,
            other => {
                return Err(format!(
                    "Expected A2L_TYPE after SYMBOL keyword, found: {:?}",
                    String::from_utf8_lossy(other)
                ));
            }
        };

        let a2l_name = a2l_name.unwrap_or(name.clone());
        Ok(Definition::Element(ElementDefinition {
            symbol_name: name,
            structure,
            config,
            a2l_name,
        }))
    }

    fn parse_instance(&mut self) -> Result<InstanceDefinition, String> {
        // Parse the INSTANCE definition
        self.require_token("INSTANCE", b"=")?;
        let name = self.get_identifier("INSTANCE")?;
        let a2l_name = self.parse_a2l_name(&[b"STRUCTURE"]);
        self.require_token("INSTANCE", b"STRUCTURE")?;
        self.require_token("INSTANCE", b"=")?;
        let structure_name = self.get_identifier("INSTANCE structure")?;

        // optional content
        let address = if self.check_next_token(b"ADDRESS") {
            let address = self.parse_tagged_uint(b"ADDRESS", "INSTANCE address")?;
            Some(address)
        } else {
            None
        };
        let (dimension, split) = self.parse_opt_dimension()?;
        let size = if self.check_next_token(b"SIZE") {
            let size = self.parse_tagged_uint(b"SIZE", "INSTANCE size")?;
            Some(size)
        } else {
            None
        };
        let group = if self.check_next_token(b"GROUP") {
            self.get_token("")?; // consume GROUP
            let group_attribute = self.parse_group_attribute()?;
            Some(group_attribute)
        } else {
            None
        };

        let mut overwrites = vec![];
        while let Some(overwrite) = self.parse_opt_overwrite()? {
            overwrites.push(overwrite);
        }

        self.require_token("INSTANCE", b"END")?;

        Ok(InstanceDefinition {
            name,
            a2l_name,
            structure_name,
            address,
            dimension,
            split,
            _size: size,
            group,
            overwrites,
        })
    }

    fn parse_sub_structure(&mut self) -> Result<Definition, String> {
        // Parse the SUB_STRUCTURE definition
        self.require_token("SUB_STRUCTURE", b"=")?;
        let name = self.get_identifier("SUB_STRUCTURE")?;
        let a2l_name = self.parse_a2l_name(&[b"STRUCTURE"]);
        let a2l_name = a2l_name.unwrap_or(name.clone());
        self.require_token("SYMBOL", b"STRUCTURE")?;
        let structure = self.parse_identifier_list("SUB_STRUCTURE structure")?;

        let data_type_struct = if self.check_next_token(b"DATA_TYPE") {
            self.require_token("SUB_STRUCTURE", b"DATA_TYPE")?;
            self.require_token("SUB_STRUCTURE", b"=")?;
            self.require_token("SUB_STRUCTURE", b"STRUCTURE")?;
            let struct_name = self.get_identifier("SUB_STRUCTURE")?;
            Some(struct_name)
        } else {
            None
        };
        let attributes = self.parse_struct_attributes()?;

        self.require_token("SUB_STRUCTURE", b"END")?;

        let structure_cfg = SubStructureCfg {
            data_type_struct,
            attributes,
        };
        Ok(Definition::Element(ElementDefinition {
            symbol_name: name,
            a2l_name,
            structure,
            config: ItemConfig::SubStructure(structure_cfg),
        }))
    }

    fn parse_var_criterion(&mut self) -> Result<VarCriterionDefinition, String> {
        // Parse the VAR_CRITERION definition
        self.require_token("VAR_CRITERION", b"=")?;
        let name = self.get_identifier("VAR_CRITERION")?;
        let description = self.parse_description("VAR_CRITERION")?;
        self.require_token("VAR_CRITERION", b"SELECTOR")?;
        self.require_token("VAR_CRITERION", b"=")?;
        let selector_type_token = self.get_token("VAR_CRITERION")?;
        let selector_type = match selector_type_token {
            b"MEASURE" => SelectorType::Measure,
            b"PARAMETER" => SelectorType::Parameter,
            other => {
                return Err(format!(
                    "Unknown VAR_CRITERION selector type: {:?}",
                    String::from_utf8_lossy(other)
                ));
            }
        };
        let selector = self.get_identifier("VAR_CRITERION")?;

        let mut variants = vec![];
        while let Ok(Some(variant)) = self.parse_opt_variant() {
            variants.push(variant);
        }

        self.require_token("VAR_CRITERION", b"END")?;

        Ok(VarCriterionDefinition {
            name,
            description,
            selector_type,
            selector,
            variants,
        })
    }

    fn parse_opt_variant(&mut self) -> Result<Option<Variant>, String> {
        if self.check_next_token(b"VARIANT") {
            self.get_token("")?; // consume "VARIANT"
            self.require_token("VARIANT", b"=")?;
            let name = self.get_identifier("VARIANT")?;
            let selector_value = self.get_uint_value("VARIANT selector value")?;
            let offset = self.get_uint_value("VARIANT offset")?;

            let variant = Variant {
                name,
                selector_value,
                offset,
            };
            Ok(Some(variant))
        } else {
            Ok(None)
        }
    }

    fn parse_write_access(&mut self) -> Option<bool> {
        match self.peek_token() {
            Some(b"WRITEABLE") => {
                let _ = self.get_token(""); // consume "WRITEABLE"
                Some(true)
            }
            Some(b"READ_ONLY") => {
                let _ = self.get_token(""); // consume "READ_ONLY"
                Some(false)
            }
            _ => None,
        }
    }

    fn parse_a2l_name(&mut self, stop_words: &[&[u8]]) -> Option<String> {
        // If the next token is not a stop word (e.g., "DATA_TYPE"), we assume it is the "a2l name"
        if let Some(token) = self.peek_token()
            && !stop_words.contains(&token)
        {
            let token = self.get_token("").ok()?;
            Some(String::from_utf8_lossy(token).to_string())
        } else {
            None
        }
    }

    fn parse_data_type(&mut self) -> Result<(DataType, Option<u64>), String> {
        self.require_token("DATA_TYPE", b"DATA_TYPE")?;
        self.require_token("DATA_TYPE", b"=")?;

        let data_type_token = self.get_token("DATA_TYPE")?;
        let data_type = match data_type_token {
            b"UBYTE" => DataType::Ubyte,
            b"SBYTE" => DataType::Sbyte,
            b"UWORD" => DataType::Uword,
            b"SWORD" => DataType::Sword,
            b"ULONG" => DataType::Ulong,
            b"SLONG" => DataType::Slong,
            b"UINT64" => DataType::AUint64,
            b"INT64" => DataType::AInt64,
            b"FLOAT" => DataType::Float32Ieee,
            b"DOUBLE" => DataType::Float64Ieee,
            other => {
                return Err(format!(
                    "Unknown DATA_TYPE: {:?}",
                    String::from_utf8_lossy(other)
                ));
            }
        };

        let bitmask = if data_type != DataType::Float32Ieee
            && data_type != DataType::Float64Ieee
            && let Ok(value) = self.get_uint_value("DATA_TYPE bitmask")
        {
            Some(value)
        } else {
            None
        };

        Ok((data_type, bitmask))
    }

    fn parse_opt_range(&mut self) -> Result<Option<(f64, f64)>, String> {
        if self.check_next_token(b"[") {
            self.get_token("")?; // consume the opening bracket
            let range_start = self.get_token("RANGE")?;

            // the components of the range description may appear separately ("123", "...", "456", "]"),
            // or in a single token ("123...456]")
            let (range_start, range_end) =
                if let Some(pos) = memchr::memmem::find(range_start, b"...") {
                    // range start and "..." are connected
                    let range_end = &range_start[pos + 3..]; // skip the "..."
                    let range_start = &range_start[..pos];
                    (range_start, range_end)
                } else {
                    // consume the separate "..."
                    self.require_token("RANGE", b"...")?;
                    let range_end = self.get_token("RANGE")?;
                    (range_start, range_end)
                };
            let range_start_val = convert_float_value(range_start, "RANGE start")?;

            let range_end = if range_end.ends_with(b"]") {
                // the range end token includes the closing bracket
                &range_end[..range_end.len() - 1]
            } else {
                // get the closing bracket separately, since it was not part of the range end token
                self.require_token("RANGE", b"]")?;
                range_end
            };
            let range_end_val = convert_float_value(range_end, "RANGE end")?;

            Ok(Some((range_start_val, range_end_val)))
        } else {
            // No range specified
            Ok(None)
        }
    }

    fn parse_attributes(&mut self) -> Result<Attributes, String> {
        let mut attributes = Attributes::default();

        while let Some(cur_token) = self.peek_token() {
            match cur_token {
                b"ADDRESS" => {
                    attributes.address = Some(self.parse_tagged_uint(b"ADDRESS", "ADDRESS")?);
                }
                b"ADDRESS_EXTENSION" => {
                    attributes.address_ext =
                        Some(self.parse_tagged_uint(b"ADDRESS_EXTENSION", "ADDRESS_EXTENSION")?);
                }
                b"ALIAS" => {
                    attributes.alias = Some(self.parse_tagged_identifier(b"ALIAS", "ALIAS")?);
                }
                b"BASE_OFFSET" => {
                    attributes.base_offset =
                        Some(self.parse_tagged_uint(b"BASE_OFFSET", "BASE_OFFSET")?);
                }
                b"BYTE_ORDER" => {
                    self.get_token("")?; // consume "BYTE_ORDER"
                    let byte_order = self.parse_byte_order_attribute()?;
                    attributes.byte_order = Some(byte_order);
                }
                b"COLOR" => {
                    attributes.color = Some(self.parse_tagged_uint(b"COLOR", "COLOR")?);
                }
                b"CONVERSION" => {
                    self.get_token("")?; // consume "CONVERSION"
                    let conversion = self.parse_conversion_attribute()?;
                    attributes.conversion = Some(conversion);
                }
                b"DESCRIPTION" => {
                    attributes.description =
                        Some(self.parse_tagged_string(b"DESCRIPTION", "DESCRIPTION")?);
                }
                b"DIMENSION" => {
                    self.get_token("")?; // consume "DIMENSION"
                    attributes.dimension = self.parse_dimension_attribute()?;
                    attributes.split = self.parse_opt_split()?;
                }
                b"EVENT" => {
                    self.get_token("")?; // consume "EVENT"
                    let event = self.parse_event_attribute()?;
                    attributes.event = Some(event);
                }
                b"GROUP" => {
                    self.get_token("")?; // consume "GROUP"
                    let group_attribute = self.parse_group_attribute()?;
                    attributes.group.push(group_attribute);
                }
                b"LAYOUT" => {
                    attributes.layout = Some(self.parse_tagged_identifier(b"LAYOUT", "LAYOUT")?);
                }
                b"UNIT" => {
                    self.get_token("")?; // consume "UNIT"
                    let unit_conversion = self.parse_unit_attribute()?;
                    attributes.conversion = Some(unit_conversion);
                }
                b"VAR_CRITERION" => {
                    attributes.var_criterion =
                        Some(self.parse_tagged_identifier(b"VAR_CRITERION", "VAR_CRITERION")?);
                }
                _ => {
                    // if any other token is found, we've reached the end of the attributes
                    break;
                }
            }
        }

        Ok(attributes)
    }

    fn parse_string_attributes(&mut self) -> Result<StringAttributes, String> {
        let mut attributes = StringAttributes::default();

        while let Some(cur_token) = self.peek_token() {
            match cur_token {
                b"ADDRESS" => {
                    attributes.address = Some(self.parse_tagged_uint(b"ADDRESS", "ADDRESS")?);
                }
                b"ADDRESS_EXTENSION" => {
                    attributes.address_ext =
                        Some(self.parse_tagged_uint(b"ADDRESS_EXTENSION", "ADDRESS_EXTENSION")?);
                }
                b"ALIAS" => {
                    attributes.alias = Some(self.parse_tagged_identifier(b"ALIAS", "ALIAS")?);
                }
                b"BASE_OFFSET" => {
                    attributes.base_offset =
                        Some(self.parse_tagged_uint(b"BASE_OFFSET", "BASE_OFFSET")?);
                }
                b"DESCRIPTION" => {
                    attributes.description =
                        Some(self.parse_tagged_string(b"DESCRIPTION", "DESCRIPTION")?);
                }
                b"DIMENSION" => {
                    self.get_token("")?; // consume "DIMENSION"
                    attributes.dimension = self.parse_dimension_attribute()?;
                    attributes.split = self.parse_opt_split()?;
                }
                b"GROUP" => {
                    self.get_token("")?; // consume "GROUP"
                    let group_attribute = self.parse_group_attribute()?;
                    attributes.group.push(group_attribute);
                }
                b"VAR_CRITERION" => {
                    attributes.var_criterion =
                        Some(self.parse_tagged_identifier(b"VAR_CRITERION", "VAR_CRITERION")?);
                }
                _ => {
                    // if any other token is found, we've reached the end of the attributes
                    break;
                }
            }
        }

        Ok(attributes)
    }

    fn parse_map_attributes(&mut self) -> Result<MapAttributes, String> {
        let mut attributes = MapAttributes::default();

        while let Some(cur_token) = self.peek_token() {
            match cur_token {
                b"ADDRESS" => {
                    attributes.address = Some(self.parse_tagged_uint(b"ADDRESS", "ADDRESS")?);
                }
                b"ADDRESS_EXTENSION" => {
                    attributes.address_ext =
                        Some(self.parse_tagged_uint(b"ADDRESS_EXTENSION", "ADDRESS_EXTENSION")?);
                }
                b"ALIAS" => {
                    attributes.alias = Some(self.parse_tagged_identifier(b"ALIAS", "ALIAS")?);
                }
                b"BASE_OFFSET" => {
                    attributes.base_offset =
                        Some(self.parse_tagged_uint(b"BASE_OFFSET", "BASE_OFFSET")?);
                }
                b"BYTE_ORDER" => {
                    self.get_token("")?; // consume "BYTE_ORDER"
                    let byte_order = self.parse_byte_order_attribute()?;
                    attributes.byte_order = Some(byte_order);
                }
                b"CONVERSION" => {
                    self.get_token("")?; // consume "CONVERSION"
                    let conversion = self.parse_conversion_attribute()?;
                    attributes.conversion = Some(conversion);
                }
                b"DESCRIPTION" => {
                    attributes.description =
                        Some(self.parse_tagged_string(b"DESCRIPTION", "DESCRIPTION")?);
                }
                b"GROUP" => {
                    self.get_token("")?; // consume "GROUP"
                    let group_attribute = self.parse_group_attribute()?;
                    attributes.group.push(group_attribute);
                }
                b"UNIT" => {
                    self.get_token("")?; // consume "UNIT"
                    let unit_conversion = self.parse_unit_attribute()?;
                    attributes.conversion = Some(unit_conversion);
                }
                b"VAR_CRITERION" => {
                    attributes.var_criterion =
                        Some(self.parse_tagged_identifier(b"VAR_CRITERION", "VAR_CRITERION")?);
                }
                _ => {
                    // if any other token is found, we've reached the end of the attributes
                    break;
                }
            }
        }

        Ok(attributes)
    }

    fn parse_struct_attributes(&mut self) -> Result<StructAttributes, String> {
        let mut attributes = StructAttributes::default();

        while let Some(cur_token) = self.peek_token() {
            match cur_token {
                b"BASE_OFFSET" => {
                    attributes.base_offset =
                        Some(self.parse_tagged_uint(b"BASE_OFFSET", "BASE_OFFSET")?);
                }
                b"DIMENSION" => {
                    self.get_token("")?; // consume "DIMENSION"
                    attributes.dimension = self.parse_dimension_attribute()?;
                    attributes.split = self.parse_opt_split()?;
                }
                b"SIZE" => {
                    attributes.size = Some(self.parse_tagged_uint(b"SIZE", "SIZE")?);
                }
                _ => {
                    // if any other token is found, we've reached the end of the attributes
                    break;
                }
            }
        }

        Ok(attributes)
    }

    fn parse_event_attribute(&mut self) -> Result<EventType, String> {
        let cmd_type_token = self.get_token("EVENT")?;
        self.require_token("EVENT", b"=")?;

        match cmd_type_token {
            b"CCP" => {
                let value = self.get_uint_value("EVENT CCP value")?;
                Ok(EventType::Ccp(value))
            }
            b"XCP" => {
                let sub_type_token = self.get_token("EVENT XCP")?;
                match sub_type_token {
                    b"FIXED" => {
                        let value = self.get_uint_value("EVENT XCP = FIXED value")?;
                        Ok(EventType::XcpFixed(value))
                    }
                    b"VARIABLE" => {
                        let value = self.get_uint_value("EVENT XCP = VARIABLE value")?;
                        let mut values = vec![value];

                        while let Ok(value) = self.get_uint_value("") {
                            values.push(value);
                        }

                        Ok(EventType::XcpVariable(values))
                    }
                    b"DEFAULT" => {
                        let value = self.get_uint_value("EVENT XCP = DEFAULT value")?;
                        Ok(EventType::XcpDefault(value))
                    }
                    other => Err(format!(
                        "Unknown XCP event category \"{}\"",
                        String::from_utf8_lossy(other)
                    )),
                }
            }
            other => Err(format!(
                "Unexpected event type {} in EVENT definition",
                String::from_utf8_lossy(other)
            )),
        }
    }

    fn parse_byte_order_attribute(&mut self) -> Result<ByteOrderEnum, String> {
        self.require_token("BYTE_ORDER", b"=")?;
        let bo_token = self.get_token("BYTE_ORDER")?;
        match bo_token {
            b"INTEL" => Ok(ByteOrderEnum::MsbLast),
            b"MOTOROLA" => Ok(ByteOrderEnum::MsbFirst),
            other => Err(format!(
                "unknown byte order {}",
                String::from_utf8_lossy(other)
            )),
        }
    }

    fn parse_opt_dimension(&mut self) -> Result<(Vec<u32>, Option<SplitType>), String> {
        if self.check_next_token(b"DIMENSION") {
            self.get_token("")?; // consume "DIMENSION"
            let dim = self.parse_dimension_attribute()?;
            let split = self.parse_opt_split()?;
            Ok((dim, split))
        } else {
            Ok((Vec::new(), None))
        }
    }

    fn parse_dimension_attribute(&mut self) -> Result<Vec<u32>, String> {
        self.require_token("DIMENSION", b"=")?;
        // at least one dimension is required
        let dim0 = self.get_uint_value("DIMENSION")?;

        let mut dim = vec![dim0];
        // try to get additional optional dimensions (up to 5 total)
        for _ in 1..5 {
            if let Ok(dim_value) = self.get_uint_value("DIMENSION") {
                dim.push(dim_value);
            } else {
                break; // no more dimensions
            }
        }

        Ok(dim)
    }

    fn parse_opt_split(&mut self) -> Result<Option<SplitType>, String> {
        if self.check_next_token(b"SPLIT") {
            self.get_token("SPLIT")?; // consume "SPLIT"
            match self.peek_token() {
                Some(b"USE") => {
                    self.get_token("")?; // consume "USE"
                    let mut split_strings = vec![];
                    while let Ok(txt) = self.get_string("") {
                        split_strings.push(txt);
                    }
                    Ok(Some(SplitType::Manual(split_strings)))
                }
                Some(b"USE_TEMPLATE") => {
                    self.get_token("")?; // consume "USE_TEMPLATE"
                    let split_template = self.get_string("SPLIT USE_TEMPLATE")?;
                    Ok(Some(SplitType::Template(split_template)))
                }
                _ => {
                    // split without manual extensions or a template: use automatic splitting
                    Ok(Some(SplitType::Auto))
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Parse the GROUP attribute
    fn parse_group_attribute(&mut self) -> Result<GroupAttribute, String> {
        // note: the caller consumes the tag, since it can be GROUP or GROUP_ASSIGNMENT depending on the context
        match self.peek_token() {
            Some(b"IN") => {
                // GROUP IN -> inputs of a named function
                self.get_token("")?; // consume "IN"
                let group_list = self.parse_identifier_list("GROUP IN")?;
                Ok(GroupAttribute::In(group_list))
            }
            Some(b"OUT") => {
                // GROUP OUT -> outputs of a named function
                self.get_token("")?; // consume "OUT"
                let group_list = self.parse_identifier_list("GROUP OUT")?;
                Ok(GroupAttribute::Out(group_list))
            }
            Some(b"DEF") => {
                // GROUP DEF -> definition of a named function
                self.get_token("")?; // consume "DEF"
                let group_list = self.parse_identifier_list("GROUP DEF")?;
                Ok(GroupAttribute::Def(group_list))
            }
            Some(b"=") => {
                // GROUP -> a true group assignment
                let group_list = self.parse_identifier_list("GROUP")?;
                Ok(GroupAttribute::Std(group_list))
            }
            _ => {
                // error
                let next_token = self.get_token("GROUP")?;
                Err(format!(
                    "Unexpected token in GROUP: {:?}",
                    String::from_utf8_lossy(next_token)
                ))
            }
        }
    }

    fn parse_conversion_attribute(&mut self) -> Result<ConversionAttribute, String> {
        // Parse the CONVERSION attribute
        self.require_token("CONVERSION attribute", b"=")?;
        let cur_token = self.get_token("CONVERSION attribute")?;
        match cur_token {
            b"LINEAR" => {
                let factor = self.get_float_value("LINEAR conversion factor")?;
                let offset = self.get_float_value("LINEAR conversion offset")?;
                let unit = self.get_string("LINEAR conversion unit")?;
                let (length, digits) = self.parse_conversion_length_digits();

                Ok(ConversionAttribute::Linear {
                    factor,
                    offset,
                    unit,
                    length,
                    digits,
                })
            }
            b"FORMULA" => {
                let formula = self.get_string("FORMULA conversion formula")?;
                // optionally: inverse formula, tagged as "INVERSE"
                let inverse_formula = if self.check_next_token(b"INVERSE") {
                    self.get_token("")?; // consume "INVERSE"
                    let inverse_formula = self.get_string("FORMULA conversion inverse formula")?;
                    Some(inverse_formula)
                } else {
                    None
                };
                let unit = self.get_string("FORMULA conversion unit")?;
                let (length, digits) = self.parse_conversion_length_digits();

                Ok(ConversionAttribute::Formula {
                    formula,
                    inverse_formula,
                    unit,
                    length,
                    digits,
                })
            }
            b"TABLE" => {
                let (rows, default_value) = self.parse_conversion_table()?;
                let format_values = if self.check_next_token(b"FORMAT") {
                    self.get_token("")?; // consume "FORMAT"
                    let length = self.get_uint_value("TABLE conversion FORMAT length")?;
                    let digits = self.get_uint_value("TABLE conversion FORMAT digits")?;
                    Some((length, digits))
                } else {
                    None
                };
                let table_conversion = ConversionAttribute::Table {
                    rows,
                    default_value,
                    format_values,
                };
                Ok(table_conversion)
            }
            name => {
                // reference to named conversion
                let name = String::from_utf8_lossy(name).to_string();
                // optional args: length and number of digits. If the name is followed by only one numerical arg, this is always the number of digits.
                // If both exist, the first is the length and the second is the number of digits.
                let (length, digits) = self.parse_conversion_length_digits();
                let reference = ConversionAttribute::Reference {
                    name,
                    length,
                    digits,
                };
                Ok(reference)
            }
        }
    }

    fn parse_conversion_table(&mut self) -> Result<(Vec<TableRow>, Option<String>), String> {
        let mut rows = vec![];

        // try to get a table row. This is either a COMPU_VTAB with (value, text) or a COMPU_VTAB_RANGE with (lower value, upper value, text)
        while let Ok(value1) = self.get_float_value("TABLE conversion row value 1") {
            if let Ok(txt) = self.get_string("TABLE conversion row text") {
                // COMPU_VTAB with (value, text)
                let row = TableRow {
                    value1,
                    value2: None,
                    text: txt,
                };
                rows.push(row);
            } else if let Ok(value2) = self.get_float_value("TABLE conversion row value 2") {
                if let Ok(txt) = self.get_string("TABLE conversion row text") {
                    // COMPU_VTAB_RANGE with (value1, value2, text)
                    let row = TableRow {
                        value1,
                        value2: Some(value2),
                        text: txt,
                    };
                    rows.push(row);
                    continue;
                }
            } else {
                // second value is neither a float nor a string. Given that the first was a float, there is no valid interpretation
                return Err(
                    "Invalid TABLE conversion row: expected a string or a second float value"
                        .to_string(),
                );
            }
        }

        let default_value = if self.check_next_token(b"DEFAULT_VALUE") {
            self.get_token("")?; // consume "DEFAULT_VALUE"
            // If there is a DEFAULT_VALUE, parse it
            let default_value = self.get_string("TABLE conversion default value")?;
            Some(default_value)
        } else {
            None
        };

        Ok((rows, default_value))
    }

    fn parse_unit_attribute(&mut self) -> Result<ConversionAttribute, String> {
        self.require_token("CONVERSION attribute", b"=")?;
        let name = self.get_string("UNIT name")?;
        let (length, digits) = self.parse_conversion_length_digits();
        let unit = ConversionAttribute::Unit {
            name,
            length,
            digits,
        };
        Ok(unit)
    }

    /// Parse the conversion length and digits.
    /// Returns a tuple of (length, digits).
    fn parse_conversion_length_digits(&mut self) -> (Option<u64>, Option<u64>) {
        let Ok(value1) = self.get_uint_value("conversion length/digits") else {
            return (None, None);
        };
        let Ok(value2) = self.get_uint_value("conversion digits") else {
            // if only one value exists, then this value represents the digits
            return (None, Some(value1));
        };
        (Some(value1), Some(value2))
    }

    fn parse_unit(&mut self, context: &str) -> Result<Option<Unit>, String> {
        // Parse the UNIT definition
        if self.check_next_token(b"UNIT") {
            self.get_token("")?; // consume "UNIT"
            self.require_token(context, b"=")?;
            let name = self.get_string(context)?;
            let length = self.get_uint_value(context)?;
            let digits = self.get_uint_value(context)?;
            let unit = Unit {
                name,
                length,
                digits,
            };
            Ok(Some(unit))
        } else {
            // No unit specified
            Ok(None)
        }
    }

    fn parse_description(&mut self, context: &str) -> Result<Option<String>, String> {
        // Parse the DESCRIPTION definition
        if self.check_next_token(b"DESCRIPTION") {
            self.get_token("")?; // consume "DESCRIPTION"
            self.require_token(context, b"=")?;
            let description = self.get_string(context)?;
            Ok(Some(description))
        } else {
            // No description specified
            Ok(None)
        }
    }

    /// Parse a list of identifiers separated by `|`.
    fn parse_identifier_list(&mut self, context: &str) -> Result<Vec<String>, String> {
        self.require_token(context, b"=")?;
        self.parse_identifier_list_value(context)
    }

    /// Parse a list of identifiers separated by `|`.
    fn parse_identifier_list_value(&mut self, context: &str) -> Result<Vec<String>, String> {
        let first_group = self.get_identifier(context)?;
        let mut groups = vec![first_group];

        while self.check_next_token(b"|") {
            self.get_token("")?; // consume "|"
            let group = self.get_identifier(context)?;
            groups.push(group);
        }

        Ok(groups)
    }

    fn parse_axis(&mut self) -> Result<AxisInfo, String> {
        self.require_token("AXIS", b"=")?;
        let type_token = self.get_token("AXIS")?;
        match type_token {
            b"STANDARD" => self.parse_std_axis(),
            b"FIX" => {
                // fix axis may use either a range [start...end] or a list of axis values
                if self.check_next_token(b"[") {
                    self.parse_fix_axis_range()
                } else {
                    self.parse_fix_axis_list()
                }
            }
            b"COMMON" => self.parse_common_axis(),
            other => Err(format!(
                "unknown AXIS type {}",
                String::from_utf8_lossy(other)
            )),
        }
    }

    fn parse_std_axis(&mut self) -> Result<AxisInfo, String> {
        let (datatype, _bitmask) = self.parse_data_type()?;
        let range = self.parse_opt_range()?;
        let extended_range = self.parse_opt_range()?;
        self.require_token("AXIS", b"DIMENSION")?;
        let dimension = self.parse_dimension_attribute()?;
        let (input_signal, input_is_instance) = self.parse_axis_input_signal()?;
        let conversion = self.parse_axis_conversion()?;

        let std_axis = AxisInfo::Standard {
            datatype,
            range,
            extended_range,
            dimension,
            input_signal,
            input_is_instance,
            conversion,
        };
        Ok(std_axis)
    }

    fn parse_fix_axis_range(&mut self) -> Result<AxisInfo, String> {
        let Some(range) = self.parse_opt_range()? else {
            return Err("Invalid range value for AXIS FIX".to_string());
        };

        let range_step = if self.check_next_token(b",") {
            self.get_token("")?; // consume ","
            let range_step = self.get_float_value("AXIS FIX range step")?;
            Some(range_step)
        } else {
            None
        };
        let (input_signal, input_is_instance) = self.parse_axis_input_signal()?;
        let conversion = self.parse_axis_conversion()?;

        Ok(AxisInfo::FixRange {
            range_min: range.0,
            range_max: range.1,
            range_step,
            input_signal,
            input_is_instance,
            conversion,
        })
    }

    fn parse_fix_axis_list(&mut self) -> Result<AxisInfo, String> {
        let value = self.get_float_value("AXIS FIX")?;
        let mut axis_points = vec![value];

        while let Ok(item) = self.get_float_value("AXIS FIX") {
            axis_points.push(item);
        }

        let (input_signal, input_is_instance) = self.parse_axis_input_signal()?;
        let conversion = self.parse_axis_conversion()?;

        Ok(AxisInfo::FixList {
            axis_points,
            input_signal,
            input_is_instance,
            conversion,
        })
    }

    fn parse_common_axis(&mut self) -> Result<AxisInfo, String> {
        let (ref_name, is_instance) = if self.check_next_token(b"INSTANCE_NAME") {
            self.get_token("")?; // consume "INSTANCE_NAME"
            let name = self.get_identifier("AXIS COMMON INSTANCE_NAME")?;
            (name, true)
        } else {
            let name = self.get_identifier("AXIS COMMON")?;
            (name, false)
        };

        let common_axis = AxisInfo::Common {
            ref_name,
            is_instance,
        };
        Ok(common_axis)
    }

    fn parse_axis_conversion(&mut self) -> Result<Option<ConversionAttribute>, String> {
        if self.check_next_token(b"CONVERSION") {
            self.get_token("")?; // consume "CONVERSION"
            let conv = self.parse_conversion_attribute()?;
            Ok(Some(conv))
        } else if self.check_next_token(b"UNIT") {
            self.get_token("")?; // consume "UNIT"
            let conv = self.parse_unit_attribute()?;
            Ok(Some(conv))
        } else {
            Ok(None)
        }
    }

    fn parse_axis_input_signal(&mut self) -> Result<(Option<String>, bool), String> {
        if self.check_next_token(b"INPUT") {
            self.get_token("")?; // consume "INPUT"
            self.require_token("INPUT", b"=")?;

            if self.check_next_token(b"INSTANCE_NAME") {
                self.get_token("")?; // consume "INSTANCE_NAME"
                let name = self.get_identifier("INPUT")?;
                Ok((Some(name), true))
            } else {
                let name = self.get_identifier("INPUT")?;
                Ok((Some(name), false))
            }
        } else {
            Ok((None, false))
        }
    }

    fn parse_layout(&mut self) -> Result<String, String> {
        self.require_token("LAYOUT", b"LAYOUT")?;
        self.require_token("LAYOUT", b"=")?;

        let layout = self.get_identifier("LAYOUT")?;
        Ok(layout)
    }

    fn parse_opt_overwrite(&mut self) -> Result<Option<Overwrite>, String> {
        let overwrite = if self.check_next_token(b"OVERWRITE") {
            self.get_token("")?; // consume "OVERWRITE"
            let element_path = self.parse_identifier_list_value("OVERWRITE")?;
            let type_token = self.get_token("OVERWRITE TYPE")?;
            let details = match type_token {
                b"CONVERSION" => {
                    let conversion = self.parse_conversion_attribute()?;
                    OverwriteSpec::Conversion(conversion)
                }
                b"DESCRIPTION" => {
                    self.require_token("OVERWRITE", b"=")?;
                    let desc = self.get_string("OVERWRITE DESCRIPTION")?;
                    OverwriteSpec::Description(desc)
                }
                b"ALIAS" => {
                    self.require_token("OVERWRITE", b"=")?;
                    let alias = self.get_identifier("OVERWRITE ALIAS")?;
                    OverwriteSpec::Alias(alias)
                }
                b"COLOR" => {
                    self.require_token("OVERWRITE", b"=")?;
                    let color = self.get_uint_value("OVERWRITE COLOR")?;
                    OverwriteSpec::Color(color)
                }
                b"GROUP" => {
                    let group = self.parse_group_attribute()?;
                    OverwriteSpec::GroupAssignment(group)
                }
                b"RANGE" => {
                    self.require_token("OVERWRITE", b"=")?;
                    let Some((range_start, range_end)) = self.parse_opt_range()? else {
                        return Err("Expected range value in OVERWRITE RANGE".into());
                    };
                    OverwriteSpec::Range(range_start, range_end)
                }
                _ => {
                    return Err(format!(
                        "Unexpected token {} in OVERWRITE",
                        String::from_utf8_lossy(type_token)
                    ));
                }
            };

            Some(Overwrite {
                element_path,
                details,
            })
        } else {
            None
        };
        Ok(overwrite)
    }

    fn parse_tagged_string(&mut self, tag: &'static [u8], context: &str) -> Result<String, String> {
        self.require_token(context, tag)?;
        self.require_token(context, b"=")?;
        let value = self.get_string(context)?;
        Ok(value)
    }

    fn parse_tagged_uint<T>(&mut self, tag: &'static [u8], context: &str) -> Result<T, String>
    where
        T: Num + std::str::FromStr + Copy + 'static,
        u64: AsPrimitive<T>,
    {
        self.require_token(context, tag)?;
        self.require_token(context, b"=")?;
        let value = self.get_uint_value(context)?;
        Ok(value)
    }

    fn parse_tagged_identifier(
        &mut self,
        tag: &'static [u8],
        context: &str,
    ) -> Result<String, String> {
        self.require_token(context, tag)?;
        self.require_token(context, b"=")?;
        let value = self.get_identifier(context)?;
        Ok(value)
    }
}

// get a float value
fn convert_float_value(token: &[u8], context: &str) -> Result<f64, String> {
    let std::borrow::Cow::Borrowed(value_string) = String::from_utf8_lossy(token) else {
        return Err(format!("Expected value in {context}, found invalid UTF-8",));
    };

    if let Ok(value) = value_string.parse::<f64>() {
        return Ok(value);
    } else if let Some(hexchars) = value_string.strip_prefix("-0x") {
        // -0x... negative hex number
        if let Ok(value) = u64::from_str_radix(hexchars, 16) {
            return Ok(-(value as f64));
        }
    } else if let Some(hexchars) = value_string.strip_prefix("0x") {
        // 0x... hex number
        if let Ok(value) = u64::from_str_radix(hexchars, 16) {
            return Ok(value as f64);
        }
    }
    Err(format!(
        "Expected float value in {context}, got {value_string}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;

    #[test]
    fn invalid_item() {
        // "empty" comment - marker only, no definition
        // this should be filtered out while scanning comments
        let input = b"//@@";
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 0);

        // if parsing fails immediately, then the result is Ok(None), since doc comments might also contain the @@ marker
        let input = b"//@@ FOO";
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);
        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_none());
    }

    #[test]
    fn parse_measure_symbol() {
        let input = br#"
        /*
        @@ SYMBOL = test_case
        @@ A2L_TYPE = MEASURE
        @@ WRITEABLE
        @@ alt_name
        @@ DATA_TYPE = UBYTE 0x3f [3...40] [0...45]
        @@ CONVERSION = LINEAR 2 3 "kkk" 8 4
        @@ DESCRIPTION = "Test description"
        @@ ALIAS = TestAlias
        @@ BASE_OFFSET = 1
        @@ GROUP = parent | TestGroup
        @@ DIMENSION = 3 4 5 SPLIT USE_TEMPLATE "._%d_[%d]bla%dblub"
        @@ ADDRESS = 0x12345678
        @@ ADDRESS_EXTENSION = 0x10
        @@ EVENT CCP = 0
        @@ COLOR = 0xFF0000
        @@ VAR_CRITERION = variant
        @@ LAYOUT = TestLayout
        @@ BYTE_ORDER = INTEL
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Symbol(SymbolDefinition {
            symbol_name,
            a2l_name,
            config: ItemConfig::Measure(measure_cfg),
        }) = definition
        else {
            panic!("Expected SymbolDefinition");
        };
        assert_eq!(symbol_name, "test_case");
        assert_eq!(a2l_name, "alt_name");
        assert_eq!(measure_cfg.attributes.address, Some(0x12345678));
        assert_eq!(measure_cfg.attributes.address_ext, Some(0x10));
        assert_eq!(
            measure_cfg.attributes.description.as_deref(),
            Some("Test description")
        );
        assert_eq!(measure_cfg.datatype, DataType::Ubyte);
        assert_eq!(measure_cfg.attributes.group.len(), 1);
        let GroupAttribute::Std(group_name) = &measure_cfg.attributes.group[0] else {
            panic!("Expected group name");
        };
        assert_eq!(group_name, &vec!["parent", "TestGroup"]);
        assert_eq!(measure_cfg.attributes.base_offset, Some(1));
        assert_eq!(measure_cfg.attributes.layout.as_deref(), Some("TestLayout"));
        assert_eq!(
            measure_cfg.attributes.var_criterion.as_deref(),
            Some("variant")
        );
        assert_eq!(measure_cfg.attributes.dimension, vec![3, 4, 5]);
        let Some(SplitType::Template(template)) = &measure_cfg.attributes.split else {
            panic!("Expected split template");
        };
        assert_eq!(template, "._%d_[%d]bla%dblub");
        let Some(ConversionAttribute::Linear {
            factor,
            offset,
            unit,
            length,
            digits,
        }) = &measure_cfg.attributes.conversion
        else {
            panic!("Expected linear conversion");
        };
        assert_eq!(*factor, 2.0);
        assert_eq!(*offset, 3.0);
        assert_eq!(unit, "kkk");
        assert_eq!(*length, Some(8));
        assert_eq!(*digits, Some(4));
        assert_eq!(measure_cfg.range, Some((3.0, 40.0)));
        assert_eq!(
            measure_cfg.attributes.byte_order,
            Some(ByteOrderEnum::MsbLast)
        );
    }

    #[test]
    fn parse_parameter_symbol() {
        let input = br#"
        /*
        @@ SYMBOL = param1
        @@ A2L_TYPE = PARAMETER
        @@ WRITEABLE
        @@ DATA_TYPE = FLOAT [0...100] [-10 ... 1000]
        @@ CONVERSION = FORMULA "x*2+3" INVERSE "(x-3)/2" "unit" 8 4
        @@ DESCRIPTION = "Parameter description"
        @@ ALIAS = ParamAlias
        @@ BASE_OFFSET = 2
        @@ GROUP IN = parent | ParamGroup
        @@ DIMENSION = 10 SPLIT USE "_a" "_b" "_c" "_d" "_e" "_f" "_g" "_h" "_i" "_j"
        @@ ADDRESS = 0x87654321
        @@ ADDRESS_EXTENSION = 0x20
        @@ EVENT XCP = FIXED 1
        @@ COLOR = 0x00FF00
        @@ VAR_CRITERION = variant_param
        @@ LAYOUT = ParamLayout
        @@ BYTE_ORDER = MOTOROLA
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Symbol(SymbolDefinition {
            symbol_name,
            a2l_name,
            config: ItemConfig::Parameter(param_cfg),
        }) = definition
        else {
            panic!("Expected SymbolDefinition");
        };
        assert_eq!(symbol_name, "param1");
        assert_eq!(a2l_name, "param1");
        assert_eq!(param_cfg.attributes.address, Some(0x87654321));
        assert_eq!(param_cfg.attributes.address_ext, Some(0x20));
        assert_eq!(
            param_cfg.attributes.description.as_deref(),
            Some("Parameter description")
        );
        assert_eq!(param_cfg.datatype, DataType::Float32Ieee);
        assert_eq!(param_cfg.attributes.group.len(), 1);
        let GroupAttribute::In(group_name) = &param_cfg.attributes.group[0] else {
            panic!("Expected group name");
        };
        assert_eq!(group_name, &vec!["parent", "ParamGroup"]);
        assert_eq!(param_cfg.attributes.base_offset, Some(2));
        assert_eq!(param_cfg.attributes.layout.as_deref(), Some("ParamLayout"));
        assert_eq!(
            param_cfg.attributes.var_criterion.as_deref(),
            Some("variant_param")
        );
        assert_eq!(param_cfg.attributes.dimension, vec![10]);
        let Some(SplitType::Manual(split_suffixes)) = &param_cfg.attributes.split else {
            panic!("Expected split template");
        };
        assert_eq!(
            split_suffixes,
            &vec!["_a", "_b", "_c", "_d", "_e", "_f", "_g", "_h", "_i", "_j"]
        );
        let Some(ConversionAttribute::Formula {
            formula,
            inverse_formula,
            unit,
            length,
            digits,
        }) = &param_cfg.attributes.conversion
        else {
            panic!("Expected formula conversion");
        };
        assert_eq!(formula, "x*2+3");
        assert_eq!(inverse_formula.as_deref(), Some("(x-3)/2"));
        assert_eq!(unit, "unit");
        assert_eq!(length, &Some(8));
        assert_eq!(digits, &Some(4));
        assert_eq!(param_cfg.range, Some((0.0, 100.0)));
        assert_eq!(param_cfg.extended_range, Some((-10.0, 1000.0)));
        assert_eq!(
            param_cfg.attributes.byte_order,
            Some(ByteOrderEnum::MsbFirst)
        );
    }

    #[test]
    fn parse_map_symbol() {
        let input = br#"
        /*
        @@ SYMBOL = map1
        @@ A2L_TYPE = MAP
        @@ WRITEABLE
        @@ DATA_TYPE = DOUBLE [0...100] [-10 ... 1000]
        @@ LAYOUT = MapLayout
        @@ CONVERSION = FORMULA "x*2+3" INVERSE "(x-3)/2" "unit" 8 4
        @@ DESCRIPTION = "Map description"
        @@ ALIAS = MapAlias
        @@ BASE_OFFSET = 2
        @@ GROUP OUT = parent | MapGroup
        @@ ADDRESS = 0x87654321
        @@ ADDRESS_EXTENSION = 0x20
        @@ VAR_CRITERION = variant_map
        @@ BYTE_ORDER = MOTOROLA
        @@ X_AXIS = STANDARD
        @@   DATA_TYPE = SBYTE
        @@   DIMENSION = 10
        @@   INPUT = InputSignal
        @@   CONVERSION = TABLE 0 "zero" 1 "one" 2 "two" DEFAULT_VALUE "other" FORMAT 6 1
        @@ Y_AXIS = FIX [0...100] , 0.5
        @@   INPUT = InputSignal2
        @@   UNIT = "unit2" 2
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Symbol(SymbolDefinition {
            symbol_name,
            a2l_name,
            config: ItemConfig::CurveMap(map_cfg),
        }) = definition
        else {
            panic!("Expected SymbolDefinition");
        };
        assert_eq!(symbol_name, "map1");
        assert_eq!(a2l_name, "map1");
        assert_eq!(map_cfg.attributes.address, Some(0x87654321));
        assert_eq!(map_cfg.attributes.address_ext, Some(0x20));
        assert_eq!(
            map_cfg.attributes.description.as_deref(),
            Some("Map description")
        );
        assert_eq!(map_cfg.datatype, DataType::Float64Ieee);
        assert_eq!(map_cfg.attributes.group.len(), 1);
        let GroupAttribute::Out(group_name) = &map_cfg.attributes.group[0] else {
            panic!("Expected group name");
        };
        assert_eq!(group_name, &vec!["parent", "MapGroup"]);
        assert_eq!(map_cfg.attributes.base_offset, Some(2));
        assert_eq!(map_cfg.layout, "MapLayout");
        assert_eq!(
            map_cfg.attributes.var_criterion.as_deref(),
            Some("variant_map")
        );
        let Some(ConversionAttribute::Formula {
            formula,
            inverse_formula,
            unit,
            length,
            digits,
        }) = &map_cfg.attributes.conversion
        else {
            panic!("Expected formula conversion");
        };
        assert_eq!(formula, "x*2+3");
        assert_eq!(inverse_formula.as_deref(), Some("(x-3)/2"));
        assert_eq!(unit, "unit");
        assert_eq!(length, &Some(8));
        assert_eq!(digits, &Some(4));
        assert_eq!(map_cfg.range, Some((0.0, 100.0)));
        assert_eq!(map_cfg.extended_range, Some((-10.0, 1000.0)));
        assert_eq!(map_cfg.attributes.byte_order, Some(ByteOrderEnum::MsbFirst));

        let AxisInfo::Standard {
            datatype,
            range,
            extended_range,
            dimension,
            input_signal,
            input_is_instance,
            conversion,
        } = *map_cfg.x_axis
        else {
            panic!("Expected standard axis info");
        };
        assert_eq!(datatype, DataType::Sbyte);
        assert_eq!(range, None);
        assert_eq!(extended_range, None);
        assert_eq!(dimension, vec![10]);
        assert_eq!(input_signal.as_deref(), Some("InputSignal"));
        assert_eq!(input_is_instance, false);
        assert!(matches!(
            conversion,
            Some(ConversionAttribute::Table { .. })
        ));
    }

    #[test]
    fn parse_axis_symbol() {
        let input = br#"
        /*
        @@ SYMBOL = x_axis
        @@ A2L_TYPE = AXIS
        @@ READ_ONLY
        @@ x_axis_a2l
        @@ DATA_TYPE = SWORD [0...100] [-10 ... 1000]
        @@ LAYOUT = AxisLayout
        @@ DIMENSION = 3
        @@ INPUT = AxisInput
        @@ CONVERSION = FORMULA "x*2+3" INVERSE "(x-3)/2" "unit" 8 4
        @@ DESCRIPTION = "Axis description"
        @@ ALIAS = AxisAlias
        @@ BASE_OFFSET = 2
        @@ GROUP OUT = parent | AxisGroup
        @@ ADDRESS = 0x87654321
        @@ ADDRESS_EXTENSION = 0x20
        @@ VAR_CRITERION = variant_axis
        @@ BYTE_ORDER = MOTOROLA
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Symbol(SymbolDefinition {
            symbol_name,
            a2l_name,
            config: ItemConfig::Axis(axis_cfg),
        }) = definition
        else {
            panic!("Expected SymbolDefinition");
        };
        assert_eq!(symbol_name, "x_axis");
        assert_eq!(a2l_name, "x_axis_a2l");
        assert_eq!(axis_cfg.datatype, DataType::Sword);
        assert_eq!(axis_cfg.range, Some((0.0, 100.0)));
        assert_eq!(axis_cfg.extended_range, Some((-10.0, 1000.0)));
        assert_eq!(axis_cfg.layout, "AxisLayout");
        assert_eq!(axis_cfg.input_signal.as_deref(), Some("AxisInput"));
        assert_eq!(axis_cfg.input_is_instance, false);
        assert_eq!(axis_cfg.dimension, vec![3]);
        assert_eq!(axis_cfg.attributes.address, Some(0x87654321));
        assert_eq!(axis_cfg.attributes.address_ext, Some(0x20));
        assert_eq!(
            axis_cfg.attributes.description.as_deref(),
            Some("Axis description")
        );
        assert_eq!(axis_cfg.attributes.group.len(), 1);
        let GroupAttribute::Out(group_name) = &axis_cfg.attributes.group[0] else {
            panic!("Expected group name");
        };
        assert_eq!(group_name, &vec!["parent", "AxisGroup"]);
        assert_eq!(axis_cfg.attributes.base_offset, Some(2));
        assert_eq!(
            axis_cfg.attributes.var_criterion.as_deref(),
            Some("variant_axis")
        );
        let Some(ConversionAttribute::Formula {
            formula,
            inverse_formula,
            unit,
            length,
            digits,
        }) = &axis_cfg.attributes.conversion
        else {
            panic!("Expected formula conversion");
        };
        assert_eq!(formula, "x*2+3");
        assert_eq!(inverse_formula.as_deref(), Some("(x-3)/2"));
        assert_eq!(unit, "unit");
        assert_eq!(length, &Some(8));
        assert_eq!(digits, &Some(4));
        assert_eq!(
            axis_cfg.attributes.byte_order,
            Some(ByteOrderEnum::MsbFirst)
        );
    }

    #[test]
    fn parse_string() {
        let input = br#"
        /*
        @@ SYMBOL = StringParameter
        @@ A2L_TYPE = STRING 100
        @@ READ_ONLY
        @@ StringParameterA2l
        @@ ADDRESS = 0x12345678
        @@ ADDRESS_EXTENSION = 0x20
        @@ ALIAS = AltName
        @@ BASE_OFFSET = 1
        @@ DESCRIPTION = "String parameter description"
        @@ GROUP = GroupName
        @@ VAR_CRITERION = Variant
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Symbol(symbol_def) = definition else {
            panic!("Expected StringDefinition");
        };
        let ItemConfig::String(string_cfg) = &symbol_def.config else {
            panic!("Expected StringDefinition");
        };
        assert_eq!(symbol_def.symbol_name, "StringParameter");
        assert_eq!(symbol_def.a2l_name, "StringParameterA2l");
        assert_eq!(string_cfg.length, 100);
        assert_eq!(
            string_cfg.attributes.description.as_deref(),
            Some("String parameter description")
        );
        assert_eq!(string_cfg.attributes.group.len(), 1);
        assert_eq!(
            string_cfg.attributes.var_criterion.as_deref(),
            Some("Variant")
        );
    }

    #[test]
    fn parse_main_group() {
        let input = br#"
        /*
        @@ MAIN_GROUP = main
        @@ DESCRIPTION = "Main group description"
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::MainGroup(group_def) = definition else {
            panic!("Expected GroupDefinition");
        };
        assert_eq!(group_def.name, "main");
        assert_eq!(
            group_def.description.as_deref(),
            Some("Main group description")
        );
    }

    #[test]
    fn parse_sub_group() {
        let input = br#"
        /*
        @@ SUB_GROUP = sub
        @@ DESCRIPTION = "Sub group description"
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::SubGroup(group_def) = definition else {
            panic!("Expected GroupDefinition");
        };
        assert_eq!(group_def.name, "sub");
        assert_eq!(
            group_def.description.as_deref(),
            Some("Sub group description")
        );
    }

    #[test]
    fn parse_linear_conversion() {
        // conversion using linear parameters
        let input = br#"
        /*
        @@ CONVERSION = LinearConversion
        @@ A2L_TYPE = LINEAR 12 3
        @@ UNIT = "unit" 5 2
        @@ DESCRIPTION = "Linear conversion"
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Conversion(conversion_def) = definition else {
            panic!("Expected ConversionDefinition");
        };
        assert_eq!(conversion_def.name, "LinearConversion");
        assert_eq!(
            conversion_def.description.as_deref(),
            Some("Linear conversion")
        );
        let Some(Unit {
            name,
            length,
            digits,
        }) = &conversion_def.unit
        else {
            panic!("Expected unit");
        };
        assert_eq!(name, "unit");
        assert_eq!(*length, 5);
        assert_eq!(*digits, 2);
        let ConversionConfig::Linear(linear_cfg) = &conversion_def.config else {
            panic!("Expected linear conversion");
        };
        assert_eq!(linear_cfg.factor, 12.0);
        assert_eq!(linear_cfg.offset, 3.0);
    }

    #[test]
    fn parse_formula_conversion() {
        // conversion using textual formulas
        let input = br#"
        /*
        @@ CONVERSION = FormulaConversion
        @@ A2L_TYPE = FORMULA "x^2+7" INVERSE "sqrt(x-7)"
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Conversion(conversion_def) = definition else {
            panic!("Expected ConversionDefinition");
        };
        assert_eq!(conversion_def.name, "FormulaConversion");
        let ConversionConfig::Formula(formula_cfg) = &conversion_def.config else {
            panic!("Expected formula conversion");
        };
        assert_eq!(formula_cfg.formula, "x^2+7");
        assert_eq!(formula_cfg.inverse_formula.as_deref(), Some("sqrt(x-7)"));
    }

    #[test]
    fn parse_table_conversion() {
        // conversion using table values
        let input = br#"
        /*
        @@ CONVERSION = TableConversion
        @@ A2L_TYPE = TABLE
        @@ 0 0 "zero"
        @@ 10 10 "ten"
        @@ 20 20 "twenty"
        @@ DEFAULT_VALUE "unknown"
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Conversion(conversion_def) = definition else {
            panic!("Expected ConversionDefinition");
        };
        assert_eq!(conversion_def.name, "TableConversion");
        let ConversionConfig::Table(table_cfg) = &conversion_def.config else {
            panic!("Expected table conversion");
        };
        assert_eq!(table_cfg.rows.len(), 3);
        assert_eq!(table_cfg.rows[0].value1, 0.0);
        assert_eq!(table_cfg.rows[0].value2, Some(0.0));
        assert_eq!(table_cfg.rows[0].text, "zero");
        assert_eq!(table_cfg.rows[1].value1, 10.0);
        assert_eq!(table_cfg.rows[1].value2, Some(10.0));
        assert_eq!(table_cfg.rows[1].text, "ten");
        assert_eq!(table_cfg.rows[2].value1, 20.0);
        assert_eq!(table_cfg.rows[2].value2, Some(20.0));
        assert_eq!(table_cfg.rows[2].text, "twenty");
        assert_eq!(table_cfg.default_value.as_deref(), Some("unknown"));
    }

    #[test]
    fn parse_element() {
        let input = br#"
        /*
        @@ ELEMENT = ElementName
        @@ STRUCTURE = abc | def
        @@ A2L_TYPE = MEASURE
        @@ DATA_TYPE = ULONG
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Element(element_def) = definition else {
            panic!("Expected ElementDefinition");
        };
        assert_eq!(element_def.symbol_name, "ElementName");
        assert_eq!(element_def.structure, vec!["abc", "def"]);
        assert!(matches!(element_def.config, ItemConfig::Measure(_)));
    }

    #[test]
    fn parse_sub_structure() {
        let input = br#"
        /*
        @@ SUB_STRUCTURE = SubStruct
        @@ STRUCTURE = abc
        @@ DATA_TYPE = STRUCTURE TypeName
        @@ DIMENSION = 3 SPLIT
        @@ BASE_OFFSET = 333
        @@ SIZE = 64
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Element(element_def) = definition else {
            panic!("Expected ElementDefinition");
        };
        let ItemConfig::SubStructure(sub_struct_def) = element_def.config else {
            panic!("Expected SubStructureDefinition");
        };
        assert_eq!(element_def.symbol_name, "SubStruct");
        assert_eq!(element_def.structure, vec!["abc"]);
        assert!(matches!(
            sub_struct_def.data_type_struct.as_deref(),
            Some("TypeName")
        ));
        assert_eq!(sub_struct_def.attributes.dimension, vec![3]);
        assert_eq!(sub_struct_def.attributes.base_offset, Some(333));
        assert_eq!(sub_struct_def.attributes.size, Some(64));
    }

    #[test]
    fn parse_instance() {
        let input = br#"
        /*
        @@ INSTANCE = InstanceName
        @@ STRUCTURE = abc
        @@ ADDRESS = 0x1234
        @@ DIMENSION = 3 4 SPLIT
        @@ SIZE = 9000
        @@ GROUP = GroupName
        @@ OVERWRITE x RANGE = [ -1 ... 1 ]
        @@ OVERWRITE y CONVERSION = LINEAR 2 3 "s"
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::Instance(instance_def) = definition else {
            panic!("Expected InstanceDefinition");
        };
        assert_eq!(instance_def.name, "InstanceName");
        assert_eq!(instance_def.structure_name, "abc");
        assert_eq!(instance_def.address, Some(0x1234));
        assert_eq!(instance_def.dimension, vec![3, 4]);
        assert_eq!(instance_def._size, Some(9000));
        assert_eq!(instance_def.overwrites.len(), 2);
    }

    #[test]
    fn parse_var_criterion() {
        let input = br#"
        /*
        @@ VAR_CRITERION = Variant
        @@ DESCRIPTION = "Variant description"
        @@ SELECTOR = MEASURE InputMeasurement
        @@   VARIANT = Apple 1 0x0
        @@   VARIANT = Orange 2 0x1000
        @@   VARIANT = Banana 3 0x2000
        @@ END
        */"#;
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let definition_tokens_vec = comment_scanner.scan_comments(input);
        assert_eq!(definition_tokens_vec.len(), 1);

        let (_offset, tokens) = &definition_tokens_vec[0];
        let definition = parse_definition(&tokens).unwrap();
        assert!(definition.is_some());
        let definition = definition.unwrap();
        let Definition::VarCriterion(var_criterion_def) = definition else {
            panic!("Expected VarCriterionDefinition");
        };
        assert_eq!(var_criterion_def.name, "Variant");
        assert_eq!(
            var_criterion_def.description.as_deref(),
            Some("Variant description")
        );
        assert_eq!(var_criterion_def.selector, "InputMeasurement");
        assert_eq!(var_criterion_def.variants.len(), 3);
        assert_eq!(var_criterion_def.variants[0].name, "Apple");
        assert_eq!(var_criterion_def.variants[1].name, "Orange");
        assert_eq!(var_criterion_def.variants[2].name, "Banana");
    }
}
