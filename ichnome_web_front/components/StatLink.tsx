import Link from "next/link";
import { uri } from "../utils/uri";

type Props = {
  workspaceName: string;
  groupName: string;
  path: string;
};

export const StatLink: React.FC<Props> = ({ workspaceName, groupName, path }) => {
  const encPath = path
    .split("/")
    .map((s) => encodeURIComponent(s))
    .join("/");
  return (
    <Link href="/[workspaceName]/stats/[groupName]/[...path]" as={uri`/${workspaceName}/stats/${groupName}/` + encPath}>
      <a>{path}</a>
    </Link>
  );
};

export default StatLink;
