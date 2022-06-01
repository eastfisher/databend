// Copyright 2021 Datafuse Labs.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use common_exception::ErrorCode;
use common_exception::Result;
use common_planners::AlterClusterKeyPlan;
use common_planners::PlanNode;
use common_planners::RenameTableEntity;
use common_planners::RenameTablePlan;
use common_tracing::tracing;
use sqlparser::ast::Expr;
use sqlparser::ast::ObjectName;

use super::analyzer_expr::ExpressionAnalyzer;
use crate::sessions::QueryContext;
use crate::sql::statements::AnalyzableStatement;
use crate::sql::statements::AnalyzedResult;

#[derive(Debug, Clone, PartialEq)]
pub struct DfAlterTable {
    pub if_exists: bool,
    pub table_name: ObjectName,
    pub action: AlterTableAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AlterTableAction {
    RenameTable(ObjectName),
    ClusterBy(Vec<Expr>),
    // TODO AddColumn etc.
}

#[async_trait::async_trait]
impl AnalyzableStatement for DfAlterTable {
    #[tracing::instrument(level = "debug", skip(self, ctx), fields(ctx.id = ctx.get_id().as_str()))]
    async fn analyze(&self, ctx: Arc<QueryContext>) -> Result<AnalyzedResult> {
        let tenant = ctx.get_tenant();
        let (catalog_name, database_name, table_name) =
            super::resolve_table(&ctx, &self.table_name, "ALTER TABLE")?;

        match &self.action {
            AlterTableAction::RenameTable(o) => {
                let mut entities = Vec::new();
                // TODO check catalog and new_catalog, cross catalogs operation not allowed
                let (_new_catalog, new_database_name, new_table_name) =
                    super::resolve_table(&ctx, o, "ALTER TABLE")?;
                entities.push(RenameTableEntity {
                    if_exists: self.if_exists,
                    catalog_name,
                    database_name,
                    table_name,
                    new_database_name,
                    new_table_name,
                });

                Ok(AnalyzedResult::SimpleQuery(Box::new(
                    PlanNode::RenameTable(RenameTablePlan { tenant, entities }),
                )))
            }
            AlterTableAction::ClusterBy(exprs) => {
                let expression_analyzer = ExpressionAnalyzer::create(ctx);
                let cluster_keys = exprs.iter().try_fold(vec![], |mut acc, k| {
                    let expr = expression_analyzer.analyze_sync(k)?;
                    acc.push(expr);
                    Ok::<_, ErrorCode>(acc)
                })?;

                Ok(AnalyzedResult::SimpleQuery(Box::new(
                    PlanNode::AlterClusterKey(AlterClusterKeyPlan {
                        tenant,
                        catalog_name,
                        database_name,
                        table_name,
                        cluster_keys,
                    }),
                )))
            }
        }
    }
}
